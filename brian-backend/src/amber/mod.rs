use std::{sync::Arc, time::Duration};

use chrono::{DateTime, FixedOffset, TimeDelta, Timelike, Utc};
use robotica_backend::{
    pipes::stateful::{create_pipe, Receiver},
    spawn,
};
use robotica_common::{datetime::utc_now, unsafe_duration, unsafe_time_delta};
use tap::Pipe;
use thiserror::Error;
use tokio::time::{interval, sleep_until, Instant, MissedTickBehavior};
use tracing::{error, info};

pub mod api;
pub mod car;
pub mod hot_water;
pub mod logging;
mod price_category;
mod private;

#[derive(Debug)]
pub struct Prices {
    pub list: Vec<api::PriceResponse>,
    pub dt: DateTime<Utc>,
    pub interval: Duration,
}

impl PartialEq for Prices {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

impl Eq for Prices {}

impl Prices {
    pub fn current(&self, dt: &DateTime<Utc>) -> Option<&api::PriceResponse> {
        get_current_price_response(&self.list, dt)
    }

    fn get_cheapest_price_for_time_delta(
        &self,
        time_delta: TimeDelta,
        now: &DateTime<Utc>,
        end_time: &DateTime<Utc>,
    ) -> Option<f32> {
        let number_of_intervals =
            private::time_delta_to_number_intervals(time_delta, self.interval);

        if number_of_intervals == 0 {
            return None;
        }

        let mut prices: Vec<_> = self
            .list
            .iter()
            .filter(|p| p.is_within_range(*now, *end_time))
            .collect();

        prices.sort_by(|a, b| f32::total_cmp(&a.per_kwh, &b.per_kwh));

        prices
            .iter()
            .take(number_of_intervals)
            .last()
            .map(|p| p.per_kwh)
    }

    fn should_power_now(
        &self,
        label: &str,
        time_delta: Option<TimeDelta>,
        now: DateTime<Utc>,
        end_time: DateTime<Utc>,
        is_on: bool,
    ) -> bool {
        let do_force = time_delta.map_or(false, |estimated_charge_time_to_min| {
            let cheapest_price = self.get_cheapest_price_for_time_delta(
                estimated_charge_time_to_min,
                &now,
                &end_time,
            );

            // If currently on we raise the threshold for the cheapest price.
            // To try to prevent cycling with fluctuating prices.
            let threshold_price = match (is_on, cheapest_price) {
                (true, Some(cheapest_price)) => Some(cheapest_price * 1.1),
                (false, Some(cheapest_price)) => Some(cheapest_price),
                _ => None,
            };

            // What is the current price?
            let current_price = self.current(&now).map(|p| p.per_kwh);
            info!("{label}: Is charging, cheapest price is {cheapest_price:?}, threshold price is {threshold_price:?}, current price is {current_price:?}");

            // Should we force charging?
            matches!((threshold_price, current_price), (Some(cheapest_price), Some(current_price)) if current_price <= cheapest_price)
        });
        do_force
    }

    fn get_weighted_price(&self, dt: DateTime<Utc>) -> Option<f32> {
        let prices = &self.list;
        let pos = prices.iter().position(|pr| pr.is_current(dt))?;

        let prefix_pos = if pos > 0 { pos - 1 } else { 0 };
        let postfix_pos = if pos + 1 < prices.len() { pos + 1 } else { pos };

        let prefix = prices[prefix_pos].per_kwh;
        let current = prices[pos].per_kwh;
        let postfix = prices[postfix_pos].per_kwh;

        let values = [prefix, current, postfix];
        let weights = [25u8, 50u8, 25u8];
        let total_weights = f32::from(weights.iter().map(|x| u16::from(*x)).sum::<u16>());

        let result = values
            .iter()
            .zip(weights.iter())
            .map(|(v, w)| v * f32::from(*w))
            .sum::<f32>()
            .pipe(|x| x / total_weights);

        info!("Get Weighted Price: {values:?} {weights:?} --> {result}");
        Some(result)
    }
}

pub struct Usage {
    pub list: Vec<api::UsageResponse>,
    pub dt: DateTime<Utc>,
}

impl PartialEq for Usage {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

impl Eq for Usage {}

const HOURS_TO_SECONDS: u16 = 3600;
const fn hours(num: u16) -> u16 {
    num * HOURS_TO_SECONDS
}

/// Error when starting the Amber service
#[derive(Error, Debug)]
pub enum Error {
    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

const ONE_DAY: TimeDelta = unsafe_time_delta!(days: 1);
const RETRY_TIME: TimeDelta = unsafe_time_delta!(minutes: 1);
const MIN_POLL_TIME: TimeDelta = unsafe_time_delta!(minutes: 1);
const MAX_POLL_TIME: TimeDelta = unsafe_time_delta!(minutes: 5);
const DEFAULT_INTERVAL: Duration = unsafe_duration!(minutes: 5);

type Outputs = (Receiver<Arc<Prices>>, Receiver<Arc<Usage>>);

pub fn run(config: api::Config) -> Result<Outputs, Error> {
    let (tx_prices, rx_prices) = create_pipe("amber_prices");
    let (tx_usage, rx_usage) = create_pipe("amber_usage");

    let nem_timezone = FixedOffset::east_opt(hours(10).into())
        .ok_or_else(|| Error::Internal("Failed to create NEM timezone".to_string()))?;

    spawn(async move {
        // Update prices maximum every 5 minutes
        let mut price_instant = Instant::now() + tokio::time::Duration::from_secs(0);

        // Update usage once an hour
        let mut usage_interval = interval(tokio::time::Duration::from_secs(hours(1).into()));
        usage_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                () = sleep_until(price_instant) => {
                    let now = utc_now();
                    let today = now.with_timezone(&nem_timezone).date_naive();
                    let yesterday = today - ONE_DAY;
                    let tomorrow = today + ONE_DAY;

                    // Get prices for the current interval.
                    let prices = {
                        let mut prices = api::get_prices(&config, yesterday, tomorrow).await;

                        if let Ok(prices) = &mut prices {
                            fix_amber_weirdness(prices);
                        }

                        prices
                    };

                    // Process the results.
                    let next_delay = match prices {
                        Ok(prices) => {

                            let update_time = get_current_price_response(&prices, &now).map_or_else(|| {
                                error!("No current price found in prices: {prices:?}");
                                // If we failed to get a current price, try again in 1 minute
                                now + RETRY_TIME
                            }, |current_price| {
                                info!("Current price: {current_price:?}");
                                current_price.end_time
                            });


                            let interval = prices.last().map_or_else(|| {
                                error!("No interval found in prices: {prices:?}");
                                // If we failed to get an interval, just use default
                                DEFAULT_INTERVAL

                            }, |last_price| {
                                // If this produces an error, end time must have been before start time!
                                (last_price.end_time - last_price.start_time).to_std().unwrap_or(DEFAULT_INTERVAL)
                            });

                            tx_prices.try_send(Arc::new(Prices {
                                list: prices,
                                dt: now,
                                interval,
                            }));

                            {
                                // How long to the current interval expires?
                                let now = utc_now();
                                let duration: TimeDelta = update_time - now;
                                info!("Next price update: {update_time:?} in {duration}");

                                // Ensure we update prices at least once once every 5 minutes.
                                duration.clamp(MIN_POLL_TIME, MAX_POLL_TIME)
                            }
                        }
                        Err(err) => {
                            error!("Failed to get prices: {}", err);
                            // If we failed to get prices, try again in 1 minute
                            RETRY_TIME
                        }
                    };

                    // Schedule the next update
                    info!("Next poll in {}", next_delay);
                    let next_delay: std::time::Duration = next_delay.to_std().unwrap_or(std::time::Duration::from_secs(300));
                    price_instant = Instant::now() + next_delay;
                }
                _ = usage_interval.tick() => {
                    // Update the amber usage once an hour.
                    let now = utc_now();
                    let today = now.with_timezone(&nem_timezone).date_naive();
                    let yesterday = today - ONE_DAY;
                    let tomorrow = today + ONE_DAY;

                    // Get usage for the current interval.
                    match api::get_usage(&config, yesterday, tomorrow).await {
                        Ok(usage) => {
                            tx_usage.try_send(Arc::new(Usage {
                                list: usage,
                                dt: now,
                            }));
                        }
                        Err(err) => {
                            error!("Failed to get usage: {}", err);
                        }
                    }
                }
            }
        }
    });

    Ok((rx_prices, rx_usage))
}

fn is_period_current(pr: &api::PriceResponse, dt: &DateTime<Utc>) -> bool {
    pr.start_time <= *dt && pr.end_time > *dt
}

fn get_current_price_response<'a>(
    prices: &'a [api::PriceResponse],
    dt: &DateTime<Utc>,
) -> Option<&'a api::PriceResponse> {
    prices.iter().find(|pr| is_period_current(pr, dt))
}

fn fix_amber_weirdness(prices: &mut [api::PriceResponse]) {
    #![allow(clippy::unwrap_used)]
    for pr in prices.iter_mut() {
        // Amber sets start time to +1 second, which is weird, and stuffs up calculations.
        // This cannot actually panic.
        pr.start_time = pr.start_time.with_second(0).unwrap();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]
    use crate::amber::api::{ChannelType, IntervalType, PeriodType, TariffInformation};
    use chrono::FixedOffset;
    use chrono::Local;
    use float_cmp::assert_approx_eq;
    use robotica_common::unsafe_duration;
    use std::time::Duration;

    use super::*;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    #[test]
    fn test_get_current_price_response() {
        let pr = |start_time: DateTime<Utc>,
                  end_time: DateTime<Utc>,
                  interval_type: IntervalType| api::PriceResponse {
            date: start_time.with_timezone(&Local).date_naive(),
            start_time,
            end_time,
            per_kwh: 0.0,
            spot_per_kwh: 0.0,
            interval_type,
            renewables: 0.0,
            duration: 0,
            channel_type: api::ChannelType::General,
            estimate: Some(false),
            spike_status: "None".to_string(),
            tariff_information: api::TariffInformation {
                period: api::PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            },
        };

        let it = |current, n: i32| match n.cmp(&current) {
            std::cmp::Ordering::Less => IntervalType::ActualInterval,
            std::cmp::Ordering::Equal => IntervalType::CurrentInterval,
            std::cmp::Ordering::Greater => IntervalType::ForecastInterval,
        };

        let prices_fn = |current| {
            vec![
                pr(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    it(current, 0),
                ),
                pr(
                    dt("2020-01-01T00:30:00Z"),
                    dt("2020-01-01T01:00:00Z"),
                    it(current, 1),
                ),
                pr(
                    dt("2020-01-01T01:00:00Z"),
                    dt("2020-01-01T01:30:00Z"),
                    it(current, 2),
                ),
            ]
        };

        let now = dt("2019-12-31T23:59:59Z");
        let prices = prices_fn(-1);
        let p = get_current_price_response(&prices, &now);
        assert!(p.is_none());

        let now = dt("2020-01-01T00:00:00Z");
        let prices = prices_fn(0);
        let p = get_current_price_response(&prices, &now).unwrap();
        assert_eq!(p.start_time, prices[0].start_time);
        assert_eq!(p.end_time, prices[0].end_time);

        let now = dt("2020-01-01T00:30:00Z");
        let prices = prices_fn(1);
        let p = get_current_price_response(&prices, &now).unwrap();
        assert_eq!(p.start_time, prices[1].start_time);
        assert_eq!(p.end_time, prices[1].end_time);

        let now = dt("2020-01-01T01:00:00Z");
        let prices = prices_fn(2);
        let p = get_current_price_response(&prices, &now).unwrap();
        assert_eq!(p.start_time, prices[2].start_time);
        assert_eq!(p.end_time, prices[2].end_time);

        let prices = prices_fn(3);
        let now = dt("2020-01-01T01:30:00Z");
        let p = get_current_price_response(&prices, &now);
        assert!(p.is_none());
    }

    #[test]
    fn test_fix_amber_weirdness() {
        let pr = |start_time: DateTime<Utc>,
                  end_time: DateTime<Utc>,
                  interval_type: IntervalType| api::PriceResponse {
            date: start_time.with_timezone(&Local).date_naive(),
            start_time,
            end_time,
            per_kwh: 0.0,
            spot_per_kwh: 0.0,
            interval_type,
            renewables: 0.0,
            duration: 0,
            channel_type: api::ChannelType::General,
            estimate: Some(false),
            spike_status: "None".to_string(),
            tariff_information: api::TariffInformation {
                period: api::PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            },
        };

        let it = |current, n: i32| match n.cmp(&current) {
            std::cmp::Ordering::Less => IntervalType::ActualInterval,
            std::cmp::Ordering::Equal => IntervalType::CurrentInterval,
            std::cmp::Ordering::Greater => IntervalType::ForecastInterval,
        };

        let prices_fn = |current| {
            vec![
                pr(
                    dt("2020-01-01T00:00:01Z"),
                    dt("2020-01-01T00:30:00Z"),
                    it(current, 0),
                ),
                pr(
                    dt("2020-01-01T00:30:01Z"),
                    dt("2020-01-01T01:00:00Z"),
                    it(current, 1),
                ),
                pr(
                    dt("2020-01-01T01:00:01Z"),
                    dt("2020-01-01T01:30:00Z"),
                    it(current, 2),
                ),
            ]
        };

        let mut prices = prices_fn(1);
        fix_amber_weirdness(&mut prices);
        assert_eq!(prices.len(), 3);
        assert_eq!(prices[0].start_time, dt("2020-01-01T00:00:00Z"));
        assert_eq!(prices[0].end_time, dt("2020-01-01T00:30:00Z"));
        assert_eq!(prices[1].start_time, dt("2020-01-01T00:30:00Z"));
        assert_eq!(prices[1].end_time, dt("2020-01-01T01:00:00Z"));
        assert_eq!(prices[2].start_time, dt("2020-01-01T01:00:00Z"));
        assert_eq!(prices[2].end_time, dt("2020-01-01T01:30:00Z"));
    }

    const INTERVAL: Duration = unsafe_duration!(minutes: 30);

    macro_rules! cheapest_price_tests {
        ($($name:ident: $start_time:expr, $end_time:expr, $period:expr, $expected_price:expr)*) => {
        $(
            #[test]
            fn $name() {
                let tariff_information = TariffInformation {
                    period: PeriodType::Peak,
                    season: None,
                    block: None,
                    demand_window: None,
                };

                let pr = |start_time: DateTime<Utc>, price| {
                    let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();
                    let date = start_time.with_timezone(&timezone).date_naive();
                    let end_time = start_time + INTERVAL;
                    api::PriceResponse {
                        date,
                        start_time,
                        end_time,
                        per_kwh: price,
                        spot_per_kwh: price,
                        interval_type: IntervalType::CurrentInterval,
                        renewables: 0.0,
                        duration: 0,
                        channel_type: ChannelType::General,
                        estimate: Some(false),
                        spike_status: "None".to_string(),
                        tariff_information: tariff_information.clone(),
                    }
                };

                let prices = vec![
                    pr(dt("2020-01-01T00:00:00Z"), 20.0),
                    pr(dt("2020-01-01T00:30:00Z"), -10.0),
                    pr(dt("2020-01-01T01:00:00Z"), 0.0),
                    pr(dt("2020-01-01T01:30:00Z"), 10.0),
                    pr(dt("2020-01-01T02:00:00Z"), 0.0),
                    pr(dt("2020-01-01T02:30:00Z"), 0.0),
                    pr(dt("2020-01-01T03:30:00Z"), -10.0),
                    pr(dt("2020-01-01T04:00:00Z"), 0.0),
                    pr(dt("2020-01-01T04:30:00Z"), 0.0),
                    pr(dt("2020-01-01T05:00:00Z"), 10.0),
                    pr(dt("2020-01-01T05:30:00Z"), -10.0),
                    pr(dt("2020-01-01T06:00:00Z"), -10.0),
                ];

                let time_delta = TimeDelta::minutes($period);

                let prices = Prices {
                    list: prices,
                    dt: dt("2020-01-01T00:00:00Z"),
                    interval: INTERVAL,
                };

                let price = prices.get_cheapest_price_for_time_delta(time_delta, &$start_time, &$end_time);
                assert_eq!(
                    price,
                    $expected_price,
                );
            }
        )*
        }
    }

    cheapest_price_tests!(test_get_price_for_cheapest_period_test_1_intervals_0: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T06:30:00Z"), 0, None);
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_1_intervals_1: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T06:30:00Z"), 30, Some(-10.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_1_intervals_2: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T06:30:00Z"), 60, Some(-10.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_1_intervals_3: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T06:30:00Z"), 90, Some(-10.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_1_intervals_4: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T06:30:00Z"), 120, Some(-10.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_1_intervals_5: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T06:30:00Z"), 150, Some(0.0));

    cheapest_price_tests!(test_get_price_for_cheapest_period_test_2_intervals_0: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T06:00:00Z"), 0, None);
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_2_intervals_1: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T06:00:00Z"), 30, Some(-10.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_2_intervals_2: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T06:00:00Z"), 60, Some(-10.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_2_intervals_3: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T06:00:00Z"), 90, Some(-10.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_2_intervals_4: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T06:00:00Z"), 120, Some(0.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_2_intervals_5: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T06:00:00Z"), 150, Some(0.0));

    cheapest_price_tests!(test_get_price_for_cheapest_period_test_3_intervals_0: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z"), 0, None);
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_3_intervals_1: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z"), 30, Some(20.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_3_intervals_2: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z"), 60, Some(20.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_3_intervals_3: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z"), 90, Some(20.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_3_intervals_4: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z"), 120, Some(20.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_3_intervals_5: dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z"), 150, Some(20.0));

    cheapest_price_tests!(test_get_price_for_cheapest_period_test_4_intervals_0: dt("2020-01-01T00:00:30Z"), dt("2020-01-01T00:29:30Z"), 0, None);
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_4_intervals_1: dt("2020-01-01T00:00:30Z"), dt("2020-01-01T00:29:30Z"), 30, Some(20.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_4_intervals_2: dt("2020-01-01T00:00:30Z"), dt("2020-01-01T00:29:30Z"), 60, Some(20.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_4_intervals_3: dt("2020-01-01T00:00:30Z"), dt("2020-01-01T00:29:30Z"), 90, Some(20.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_4_intervals_4: dt("2020-01-01T00:00:30Z"), dt("2020-01-01T00:29:30Z"), 120, Some(20.0));
    cheapest_price_tests!(test_get_price_for_cheapest_period_test_4_intervals_5: dt("2020-01-01T00:00:30Z"), dt("2020-01-01T00:29:30Z"), 150, Some(20.0));

    fn pr(
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        interval_type: IntervalType,
        cost: f32,
    ) -> api::PriceResponse {
        api::PriceResponse {
            date: start_time.with_timezone(&Local).date_naive(),
            start_time,
            end_time,
            per_kwh: cost,
            spot_per_kwh: 0.0,
            interval_type,
            renewables: 0.0,
            duration: 0,
            channel_type: api::ChannelType::General,
            estimate: Some(false),
            spike_status: "None".to_string(),
            tariff_information: api::TariffInformation {
                period: api::PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            },
        }
    }

    fn pr_list_descending(cost: f32) -> Vec<api::PriceResponse> {
        let time = dt("2020-01-01T00:00:00Z");

        (0i8..48i8)
            .map(|i| {
                let i64 = i64::from(i);
                let f32 = f32::from(i);
                pr(
                    time + TimeDelta::minutes(i64 * 30),
                    time + TimeDelta::minutes((i64 + 1) * 30),
                    IntervalType::ForecastInterval,
                    f32.mul_add(-0.5, cost),
                )
            })
            // .map(|p| {
            //     debug!("{:?}", p);
            //     p
            // })
            .collect::<Vec<api::PriceResponse>>()
    }

    // test prices.should_power_now
    #[rstest::rstest]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T02:00:00Z"),
        TimeDelta::minutes(30),
        false,
        false
    )]
    #[case(
        dt("2020-01-01T01:30:00Z"),
        dt("2020-01-01T02:00:00Z"),
        TimeDelta::minutes(30),
        false,
        true
    )]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T02:00:00Z"),
        TimeDelta::minutes(30),
        true,
        true
    )]
    #[case(
        dt("2020-01-01T01:30:00Z"),
        dt("2020-01-01T02:00:00Z"),
        TimeDelta::minutes(30),
        true,
        true
    )]
    fn test_should_power_now(
        #[case] now: DateTime<Utc>,
        #[case] end_time: DateTime<Utc>,
        #[case] time_delta: TimeDelta,
        #[case] is_on: bool,
        #[case] expected: bool,
    ) {
        let prices = Prices {
            list: pr_list_descending(50.0),
            // list: vec![
            //     api::PriceResponse {
            //         date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            //         start_time: dt("2020-01-01T00:00:00Z"),
            //         end_time: dt("2020-01-01T00:30:00Z"),
            //         per_kwh: 20.0,
            //         spot_per_kwh: 20.0,
            //         interval_type: IntervalType::CurrentInterval,
            //         renewables: 0.0,
            //         duration: 0,
            //         channel_type: ChannelType::General,
            //         estimate: Some(false),
            //         spike_status: "None".to_string(),
            //         tariff_information: TariffInformation {
            //             period: PeriodType::Peak,
            //             season: None,
            //             block: None,
            //             demand_window: None,
            //         },
            //     },
            //     api::PriceResponse {
            //         date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            //         start_time: dt("2020-01-01T00:30:00Z"),
            //         end_time: dt("2020-01-01T01:00:00Z"),
            //         per_kwh: -10.0,
            //         spot_per_kwh: -10.0,
            //         interval_type: IntervalType::CurrentInterval,
            //         renewables: 0.0,
            //         duration: 0,
            //         channel_type: ChannelType::General,
            //         estimate: Some(false),
            //         spike_status: "None".to_string(),
            //         tariff_information: TariffInformation {
            //             period: PeriodType::Peak,
            //             season: None,
            //             block: None,
            //             demand_window: None,
            //         },
            //     },
            // ],
            dt: dt("2020-01-01T00:00:00Z"),
            interval: INTERVAL,
        };

        assert_eq!(
            prices.should_power_now("test", Some(time_delta), now, end_time, is_on),
            expected
        );
    }

    #[test]
    fn test_get_weighted_price() {
        let pr = |start_time: DateTime<Utc>,
                  end_time: DateTime<Utc>,
                  price,
                  interval_type: IntervalType| api::PriceResponse {
            date: start_time.with_timezone(&Local).date_naive(),
            start_time,
            end_time,
            per_kwh: price,
            spot_per_kwh: price,
            interval_type,
            renewables: 0.0,
            duration: 0,
            channel_type: api::ChannelType::General,
            estimate: Some(false),
            spike_status: "None".to_string(),
            tariff_information: api::TariffInformation {
                period: api::PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            },
        };

        let it = |current, n: i32| match n.cmp(&current) {
            std::cmp::Ordering::Less => IntervalType::ActualInterval,
            std::cmp::Ordering::Equal => IntervalType::CurrentInterval,
            std::cmp::Ordering::Greater => IntervalType::ForecastInterval,
        };

        let prices_fn = |current| {
            let prices = vec![
                pr(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    1.0,
                    it(current, 0),
                ),
                pr(
                    dt("2020-01-01T00:30:00Z"),
                    dt("2020-01-01T01:00:00Z"),
                    2.0,
                    it(current, 1),
                ),
                pr(
                    dt("2020-01-01T01:00:00Z"),
                    dt("2020-01-01T01:30:00Z"),
                    4.0,
                    it(current, 2),
                ),
            ];
            Prices {
                list: prices,
                dt: dt("2020-01-01T00:00:00Z"),
                interval: INTERVAL,
            }
        };

        let now = dt("2020-01-01T00:00:00Z");
        let prices = prices_fn(0);
        let p = prices.get_weighted_price(now).unwrap();
        assert_approx_eq!(f32, p, 1.25);

        let now = dt("2020-01-01T00:30:00Z");
        let prices = prices_fn(1);
        let p = prices.get_weighted_price(now).unwrap();
        assert_approx_eq!(f32, p, 2.25);

        let now = dt("2020-01-01T01:00:00Z");
        let prices = prices_fn(2);
        let p = prices.get_weighted_price(now).unwrap();
        assert_approx_eq!(f32, p, 3.5);

        let now = dt("2020-01-01T01:30:00Z");
        let prices = prices_fn(3);
        let p = prices.get_weighted_price(now);
        assert!(p.is_none());
    }
}
