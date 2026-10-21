use std::{sync::Arc, time::Duration};

use chrono::{DateTime, FixedOffset, TimeDelta, Timelike, Utc};
use robotica_common::datetime::utc_now;
use robotica_common::robotica::entities::Id;
use robotica_macro::{duration_constant, time_delta_constant};
use robotica_tokio::{
    pipes::{
        stateful::{create_pipe, Receiver},
        stateless,
    },
    spawn,
};
use serde::ser::{Serialize, Serializer};
use tap::Pipe;
use thiserror::Error;
use tokio::time::{interval, sleep_until, Instant, MissedTickBehavior};
use tracing::{error, info};
use tracing_log_error::log_error;

pub mod api;
pub mod car;
mod combined;
pub mod logging;
mod plan;
mod private;
pub mod rules;
mod user_plan;
pub mod water_heater;

// pub use user_plan::MaybeUserPlan;

#[derive(Debug, PartialEq)]
pub struct Prices {
    general: Vec<api::PriceResponse>,
    controlled_load: Vec<api::PriceResponse>,
    feed_in: Vec<api::PriceResponse>,
    pub interval: Duration,
}

impl Prices {
    fn get_vec_for_type(
        prices: &[api::PriceResponse],
        channel_type: api::ChannelType,
    ) -> Vec<api::PriceResponse> {
        let count = prices
            .iter()
            .filter(|pr| pr.channel_type == channel_type)
            .count();
        Vec::with_capacity(count)
    }

    pub fn new(id: &Id, prices: Vec<api::PriceResponse>) -> Self {
        let interval = prices.last().map_or_else(
            || {
                error!(%id, "No interval found in prices: {prices:?}");
                // If we failed to get an interval, just use default
                DEFAULT_INTERVAL
            },
            |last_price| {
                // If this produces an error, end time must have been before start time!
                (last_price.end_time - last_price.start_time)
                    .to_std()
                    .unwrap_or(DEFAULT_INTERVAL)
            },
        );

        let mut general = Self::get_vec_for_type(&prices, api::ChannelType::General);
        let mut controlled_load = Self::get_vec_for_type(&prices, api::ChannelType::ControlledLoad);
        let mut feed_in = Self::get_vec_for_type(&prices, api::ChannelType::FeedIn);

        for pr in prices {
            match pr.channel_type {
                api::ChannelType::General => general.push(pr),
                api::ChannelType::ControlledLoad => controlled_load.push(pr),
                api::ChannelType::FeedIn => feed_in.push(pr),
            }
        }

        Self {
            general,
            controlled_load,
            feed_in,
            interval,
        }
    }

    pub fn general(&self) -> GeneralPrices<'_> {
        GeneralPrices {
            list: &self.general,
            interval: self.interval,
        }
    }

    pub fn current(&self, dt: &DateTime<Utc>) -> Vec<&api::PriceResponse> {
        let mut result = Vec::with_capacity(3);
        if let Some(pr) = self.general.iter().find(|pr| pr.is_current(*dt)) {
            result.push(pr);
        }
        if let Some(pr) = self.controlled_load.iter().find(|pr| pr.is_current(*dt)) {
            result.push(pr);
        }
        if let Some(pr) = self.feed_in.iter().find(|pr| pr.is_current(*dt)) {
            result.push(pr);
        }
        result
    }

    pub fn combined_iter(&self) -> impl Iterator<Item = &api::PriceResponse> {
        self.general
            .iter()
            .chain(self.controlled_load.iter())
            .chain(self.feed_in.iter())
    }
}

/// A view over [`Prices`] that serializes as a single combined list.
///
/// This avoids cloning every [`api::PriceResponse`] when publishing prices to
/// MQTT.
#[derive(Clone, Debug)]
pub struct CombinedPrices(pub Arc<Prices>);

impl PartialEq for CombinedPrices {
    fn eq(&self, other: &Self) -> bool {
        self.0.combined_iter().eq(other.0.combined_iter())
    }
}

impl Serialize for CombinedPrices {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_seq(self.0.combined_iter())
    }
}

#[derive(Debug)]
pub struct GeneralPrices<'a> {
    pub list: &'a [api::PriceResponse],
    pub interval: Duration,
}

impl GeneralPrices<'_> {
    pub fn current(&self, dt: &DateTime<Utc>) -> Option<&api::PriceResponse> {
        get_current_price_response(self.list, dt)
    }

    fn get_next_period(
        &self,
        id: &Id,
        now: chrono::DateTime<Utc>,
    ) -> Option<chrono::DateTime<Utc>> {
        let Ok(interval) = i64::try_from(self.interval.as_secs()) else {
            error!(
                %id,
                "Failed to convert interval to i64: {}",
                self.interval.as_secs()
            );
            return None;
        };

        let timestamp = now.timestamp();
        let value = timestamp + interval - timestamp % interval;
        DateTime::from_timestamp(value, 0)
    }

    fn get_weighted_price(&self, id: &Id, dt: DateTime<Utc>) -> Option<f32> {
        let prices = &self.list;
        let pos = prices.iter().position(|pr| pr.is_current(dt))?;

        let prefix_pos = if pos > 0 { pos - 1 } else { 0 };
        let postfix_pos = if pos + 1 < prices.len() { pos + 1 } else { pos };

        let prefix = prices[prefix_pos].effective_per_kwh();
        let current = prices[pos].effective_per_kwh();
        let postfix = prices[postfix_pos].effective_per_kwh();

        let values = [prefix, current, postfix];
        let weights = [25u8, 50u8, 25u8];
        let total_weights = f32::from(weights.iter().map(|x| u16::from(*x)).sum::<u16>());

        let result = values
            .iter()
            .zip(weights.iter())
            .map(|(v, w)| v * f32::from(*w))
            .sum::<f32>()
            .pipe(|x| x / total_weights);

        info!(%id, "Get Weighted Price: {values:?} {weights:?} --> {result}");
        Some(result)
    }

    fn find(&self, now: chrono::DateTime<Utc>) -> Option<&api::PriceResponse> {
        self.list.iter().find(|pr| pr.is_current(now))
    }
}

pub struct Usage {
    pub list: Vec<api::UsageResponse>,
}

impl PartialEq for Usage {
    fn eq(&self, other: &Self) -> bool {
        self.list == other.list
    }
}

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

const ONE_DAY: TimeDelta = time_delta_constant!(1 days);
const RETRY_TIME: TimeDelta = time_delta_constant!(1 minutes);
const POLL_TIME: TimeDelta = time_delta_constant!(5 minutes);
const DEFAULT_INTERVAL: Duration = duration_constant!(5 minutes);

/// Outputs of the Amber service.
pub struct Outputs {
    /// Current prices.
    ///
    /// The value only changes when the price data actually changes;
    /// identical poll results are deduplicated by the pipe.
    pub prices: Receiver<Arc<Prices>>,
    /// The time each new pricing interval begins.
    ///
    /// Fires at each interval boundary, even if prices are unchanged or the
    /// Amber API is temporarily unavailable, so consumers can re-evaluate on
    /// interval changes. When the price data runs out it fires once more,
    /// then stops until the poll loop fetches fresh prices.
    pub tick: stateless::Receiver<DateTime<Utc>>,
    /// Usage data, updated hourly.
    pub usage: Receiver<Arc<Usage>>,
}

pub fn run(id: &Id, config: api::Config) -> Result<Outputs, Error> {
    let (tx_prices, rx_prices) = create_pipe("amber_prices");
    let (tx_usage, rx_usage) = create_pipe("amber_usage");
    let (tx_tick, rx_tick) = stateless::create_pipe("amber_interval_boundary");

    let nem_timezone = FixedOffset::east_opt(hours(10).into())
        .ok_or_else(|| Error::Internal("Failed to create NEM timezone".to_string()))?;

    let id = id.clone();

    spawn(async move {
        // Update prices maximum every 5 minutes
        let mut price_instant = Instant::now() + tokio::time::Duration::from_secs(0);

        // The deadline of the next pricing interval boundary, or [`None`]
        // while there is no price data covering a boundary. Armed whenever
        // fresh prices arrive.
        let mut tick_instant: Option<Instant> = None;

        // The last prices successfully fetched from Amber. Used to compute the
        // next boundary when polls fail.
        let mut last_prices: Option<Arc<Prices>> = None;

        // Update usage once an hour
        let mut usage_interval = interval(tokio::time::Duration::from_secs(hours(1).into()));
        usage_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // {
        //     #[allow(clippy::unwrap_used)]
        //     let start_date = NaiveDate::from_ymd_opt(2024, 1, 25).unwrap();
        //     #[allow(clippy::unwrap_used)]
        //     let end_date = NaiveDate::from_ymd_opt(2024, 3, 17).unwrap();

        //     // #[allow(clippy::unwrap_used)]
        //     // let start_date = NaiveDate::from_ymd_opt(2024, 6, 2).unwrap();
        //     // #[allow(clippy::unwrap_used)]
        //     // let end_date = NaiveDate::from_ymd_opt(2024, 6, 11).unwrap();

        //     error!("Getting usage for month");
        //     match api::get_usage(&config, start_date, end_date).await {
        //         Ok(usage) => {
        //             error!("Got usage for month: {usage:#?}");
        //             tx_usage.try_send(Arc::new(Usage {
        //                 list: usage,
        //                 dt: utc_now(),
        //             }));
        //         }
        //         Err(err) => {
        //             error!("Failed to get usage for month: {err}");
        //         }
        //     }
        //     error!("Got usage for month");
        //     tokio::time::sleep(Duration::from_secs(30)).await;
        // }

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
                            let prices = Arc::new(Prices::new(&id, prices));
                            tx_prices.try_send(prices.clone());
                            last_prices = Some(prices);

                            // Arm the interval boundary tick at the end of
                            // the current period.
                            tick_instant =
                                get_current_period_end(last_prices.as_deref(), utc_now())
                                    .map(boundary_instant);

                            POLL_TIME
                        }
                        Err(err) => {
                            log_error!(err, %id, "Failed to get prices");
                            // If we failed to get prices, try again in 1 minute
                            RETRY_TIME
                        }
                    };

                    // Schedule the next update
                    info!(%id, "Next poll in {}", next_delay);
                    let next_delay: std::time::Duration = next_delay.to_std().unwrap_or(std::time::Duration::from_mins(5));
                    price_instant = Instant::now() + next_delay;
                }
                () = async {
                    match tick_instant {
                        Some(instant) => sleep_until(instant).await,
                        None => std::future::pending().await,
                    }
                } => {
                    // We have entered a new pricing interval. Notify
                    // consumers so they can re-evaluate, even if prices are
                    // unchanged or the Amber API is unavailable.
                    let now = utc_now();
                    info!(%id, boundary = %now, "Pricing interval boundary");
                    tx_tick.try_send(now);

                    // Arm the next tick at the end of the current period;
                    // disarms (None) when the price data runs out, until
                    // the poll loop fetches fresh prices.
                    tick_instant =
                        get_current_period_end(last_prices.as_deref(), now)
                            .map(boundary_instant);
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
                            }));
                        }
                        Err(err) => {
                            log_error!(err, %id, "Failed to get usage");
                        }
                    }
                }
            }
        }
    });

    Ok(Outputs {
        prices: rx_prices,
        tick: rx_tick,
        usage: rx_usage,
    })
}

fn is_period_current(pr: &api::PriceResponse, dt: &DateTime<Utc>) -> bool {
    pr.start_time <= *dt && pr.end_time > *dt
}

/// Get the end time of the pricing interval that covers `now`.
///
/// Returns [`None`] when the price list doesn't cover `now`: the next
/// interval boundary is then unknown, and the poll loop retries the fetch
/// until fresh prices cover `now` again.
fn get_current_period_end(prices: Option<&Prices>, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    prices
        .and_then(|prices| get_current_price_response(&prices.general, &now))
        .map(|pr| pr.end_time)
}

/// Convert a wall clock time to a timer deadline.
fn boundary_instant(boundary: DateTime<Utc>) -> Instant {
    let delta = boundary - utc_now();
    Instant::now() + delta.to_std().unwrap_or_default()
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
    use crate::amber::api::IntervalType;
    use chrono::Local;
    use float_cmp::assert_approx_eq;
    use std::time::Duration;

    use super::*;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    #[rstest::rstest]
    #[case(dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z"))]
    #[case(dt("2020-01-01T00:00:01Z"), dt("2020-01-01T00:30:00Z"))]
    #[case(dt("2020-01-01T00:00:00.55Z"), dt("2020-01-01T00:30:00Z"))]
    #[case(dt("2020-01-01T00:10:00Z"), dt("2020-01-01T00:30:00Z"))]
    #[case(dt("2020-01-01T00:29:00Z"), dt("2020-01-01T00:30:00Z"))]
    #[case(dt("2020-01-01T00:30:00Z"), dt("2020-01-01T01:00:00Z"))]
    fn test_get_next_period(#[case] now: DateTime<Utc>, #[case] expected: DateTime<Utc>) {
        let prices = GeneralPrices {
            list: &[],
            interval: INTERVAL,
        };
        let id = Id::new("test").unwrap();

        let result = prices.get_next_period(&id, now).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_get_current_period_end_with_current_period() {
        let id = Id::new("test").unwrap();

        let prices = Prices::new(
            &id,
            vec![
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    1.0,
                    api::ChannelType::General,
                ),
                pr_channel(
                    dt("2020-01-01T00:30:00Z"),
                    dt("2020-01-01T01:00:00Z"),
                    2.0,
                    api::ChannelType::General,
                ),
            ],
        );

        // The current period's end time is the next boundary.
        let now = dt("2020-01-01T00:10:00Z");
        assert_eq!(
            get_current_period_end(Some(&prices), now),
            Some(dt("2020-01-01T00:30:00Z"))
        );

        // Once the boundary has passed, the next period's end time is used.
        let now = dt("2020-01-01T00:30:00Z");
        assert_eq!(
            get_current_period_end(Some(&prices), now),
            Some(dt("2020-01-01T01:00:00Z"))
        );
    }

    #[test]
    fn test_get_current_period_end_with_stale_prices() {
        let id = Id::new("test").unwrap();

        // The price list expired long ago; the next boundary is unknown.
        let prices = Prices::new(
            &id,
            vec![pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                1.0,
                api::ChannelType::General,
            )],
        );

        let now = dt("2020-01-01T05:10:00Z");
        assert_eq!(get_current_period_end(Some(&prices), now), None);
    }

    #[test]
    fn test_get_current_period_end_without_prices() {
        let now = dt("2020-01-01T00:10:00Z");
        assert_eq!(get_current_period_end(None, now), None);
    }

    #[rstest::rstest]
    #[case(dt("2020-01-01T00:00:00Z"), 1.0)]
    #[case(dt("2020-01-01T00:10:00Z"), 1.0)]
    #[case(dt("2020-01-01T00:29:00Z"), 1.0)]
    #[case(dt("2020-01-01T00:30:00Z"), 2.0)]
    fn test_find(#[case] now: DateTime<Utc>, #[case] expected: f32) {
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
            nem_time: start_time,
            descriptor: api::Descriptor::Neutral,
            estimate: Some(false),
            spike_status: "None".to_string(),
            advanced_price: None,
            tariff_information: Some(api::TariffInformation {
                period: api::PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            }),
        };

        let prices = vec![
            pr(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                1.0,
                IntervalType::ActualInterval,
            ),
            pr(
                dt("2020-01-01T00:30:00Z"),
                dt("2020-01-01T01:00:00Z"),
                2.0,
                IntervalType::ActualInterval,
            ),
            pr(
                dt("2020-01-01T01:00:00Z"),
                dt("2020-01-01T01:30:00Z"),
                4.0,
                IntervalType::ActualInterval,
            ),
        ];
        let prices = GeneralPrices {
            list: &prices,
            interval: INTERVAL,
        };

        let result = prices.find(now).unwrap().per_kwh;
        assert_approx_eq!(f32, result, expected);
    }

    #[rstest::rstest]
    #[case(dt("2020-01-01T00:00:00Z"))]
    #[case(dt("2020-01-01T00:10:00Z"))]
    #[case(dt("2020-01-01T00:29:00Z"))]
    fn test_find_none(#[case] now: DateTime<Utc>) {
        let prices = GeneralPrices {
            list: &[],
            interval: INTERVAL,
        };

        let result = prices.find(now);
        assert!(result.is_none());
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
            nem_time: start_time,
            descriptor: api::Descriptor::Neutral,
            estimate: Some(false),
            spike_status: "None".to_string(),
            advanced_price: None,
            tariff_information: Some(api::TariffInformation {
                period: api::PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            }),
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
            nem_time: start_time,
            descriptor: api::Descriptor::Neutral,
            estimate: Some(false),
            spike_status: "None".to_string(),
            advanced_price: None,
            tariff_information: Some(api::TariffInformation {
                period: api::PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            }),
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

    const INTERVAL: Duration = duration_constant!(30 minutes);

    #[test]
    fn test_get_weighted_price() {
        let id = Id::new("test").unwrap();

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
            nem_time: start_time,
            descriptor: api::Descriptor::Neutral,
            estimate: Some(false),
            spike_status: "None".to_string(),
            advanced_price: None,
            tariff_information: Some(api::TariffInformation {
                period: api::PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            }),
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
            ]
        };

        let now = dt("2020-01-01T00:00:00Z");
        let prices = prices_fn(0);
        let prices = GeneralPrices {
            list: &prices,
            interval: INTERVAL,
        };
        let p = prices.get_weighted_price(&id, now).unwrap();
        assert_approx_eq!(f32, p, 1.25);

        let now = dt("2020-01-01T00:30:00Z");
        let prices = prices_fn(1);
        let prices = GeneralPrices {
            list: &prices,
            interval: INTERVAL,
        };
        let p = prices.get_weighted_price(&id, now).unwrap();
        assert_approx_eq!(f32, p, 2.25);

        let now = dt("2020-01-01T01:00:00Z");
        let prices = prices_fn(2);
        let prices = GeneralPrices {
            list: &prices,
            interval: INTERVAL,
        };
        let p = prices.get_weighted_price(&id, now).unwrap();
        assert_approx_eq!(f32, p, 3.5);

        let now = dt("2020-01-01T01:30:00Z");
        let prices = prices_fn(3);
        let prices = GeneralPrices {
            list: &prices,
            interval: INTERVAL,
        };
        let p = prices.get_weighted_price(&id, now);
        assert!(p.is_none());
    }

    fn pr_channel(
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        per_kwh: f32,
        channel_type: api::ChannelType,
    ) -> api::PriceResponse {
        api::PriceResponse {
            date: start_time.with_timezone(&Local).date_naive(),
            start_time,
            end_time,
            per_kwh,
            spot_per_kwh: per_kwh,
            interval_type: IntervalType::ActualInterval,
            renewables: 0.0,
            duration: 30,
            channel_type,
            nem_time: start_time,
            descriptor: api::Descriptor::Neutral,
            estimate: Some(false),
            spike_status: "None".to_string(),
            advanced_price: None,
            tariff_information: Some(api::TariffInformation {
                period: api::PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            }),
        }
    }

    #[test]
    fn test_prices_new_partitions_by_channel_type() {
        let id = Id::new("test").unwrap();

        // Input interleaves channel types to prove they are partitioned.
        let prices = vec![
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                1.0,
                api::ChannelType::General,
            ),
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                2.0,
                api::ChannelType::ControlledLoad,
            ),
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                3.0,
                api::ChannelType::FeedIn,
            ),
            pr_channel(
                dt("2020-01-01T00:30:00Z"),
                dt("2020-01-01T01:00:00Z"),
                4.0,
                api::ChannelType::General,
            ),
            pr_channel(
                dt("2020-01-01T00:30:00Z"),
                dt("2020-01-01T01:00:00Z"),
                5.0,
                api::ChannelType::ControlledLoad,
            ),
            pr_channel(
                dt("2020-01-01T00:30:00Z"),
                dt("2020-01-01T01:00:00Z"),
                6.0,
                api::ChannelType::FeedIn,
            ),
        ];
        let prices = Prices::new(&id, prices);

        // The interval is derived from the last price's start/end times.
        assert_eq!(prices.interval, INTERVAL);

        // The general view exposes only the general prices, in input order.
        let general = prices.general();
        assert_eq!(general.list.len(), 2);
        assert!(general
            .list
            .iter()
            .all(|pr| pr.channel_type == api::ChannelType::General));
        assert_approx_eq!(f32, general.list[0].per_kwh, 1.0);
        assert_approx_eq!(f32, general.list[1].per_kwh, 4.0);

        // The combined iterator yields every price, grouped by channel type.
        let channel_types: Vec<_> = prices.combined_iter().map(|pr| pr.channel_type).collect();
        assert_eq!(
            channel_types,
            vec![
                api::ChannelType::General,
                api::ChannelType::General,
                api::ChannelType::ControlledLoad,
                api::ChannelType::ControlledLoad,
                api::ChannelType::FeedIn,
                api::ChannelType::FeedIn,
            ]
        );
    }

    #[test]
    fn test_prices_new_interval_fallbacks_to_default() {
        let id = Id::new("test").unwrap();

        // An empty price list has no interval to derive one from.
        let prices = Prices::new(&id, Vec::new());
        assert_eq!(prices.interval, DEFAULT_INTERVAL);

        // A last price with end_time before start_time yields a negative
        // interval, which must also fall back to the default.
        let last = pr_channel(
            dt("2020-01-01T00:30:00Z"),
            dt("2020-01-01T00:00:00Z"),
            1.0,
            api::ChannelType::General,
        );
        let prices = Prices::new(&id, vec![last]);
        assert_eq!(prices.interval, DEFAULT_INTERVAL);
    }

    #[test]
    fn test_prices_general() {
        let id = Id::new("test").unwrap();
        let prices = Prices::new(
            &id,
            vec![
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    1.0,
                    api::ChannelType::General,
                ),
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    2.0,
                    api::ChannelType::ControlledLoad,
                ),
                pr_channel(
                    dt("2020-01-01T00:30:00Z"),
                    dt("2020-01-01T01:00:00Z"),
                    4.0,
                    api::ChannelType::General,
                ),
            ],
        );

        let general = prices.general();

        // The view shares the interval and exposes only general prices.
        assert_eq!(general.interval, prices.interval);
        assert_eq!(general.list.len(), 2);

        // The view supports looking up the current general price.
        let current = general.current(&dt("2020-01-01T00:15:00Z")).unwrap();
        assert_eq!(current.channel_type, api::ChannelType::General);
        assert_approx_eq!(f32, current.per_kwh, 1.0);
    }

    #[test]
    fn test_prices_current() {
        let id = Id::new("test").unwrap();

        let prices = Prices::new(
            &id,
            vec![
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    1.0,
                    api::ChannelType::General,
                ),
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    2.0,
                    api::ChannelType::ControlledLoad,
                ),
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    3.0,
                    api::ChannelType::FeedIn,
                ),
                pr_channel(
                    dt("2020-01-01T00:30:00Z"),
                    dt("2020-01-01T01:00:00Z"),
                    4.0,
                    api::ChannelType::General,
                ),
            ],
        );

        // At most one current price per channel type, in combined order.
        let current = prices.current(&dt("2020-01-01T00:15:00Z"));
        assert_eq!(current.len(), 3);
        assert_eq!(current[0].channel_type, api::ChannelType::General);
        assert_eq!(current[1].channel_type, api::ChannelType::ControlledLoad);
        assert_eq!(current[2].channel_type, api::ChannelType::FeedIn);

        // Only the general channel has a current interval.
        let current = prices.current(&dt("2020-01-01T00:45:00Z"));
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].channel_type, api::ChannelType::General);
        assert_approx_eq!(f32, current[0].per_kwh, 4.0);

        // Overlapping current intervals in one channel yield only the first.
        let overlapping = Prices::new(
            &id,
            vec![
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:40:00Z"),
                    1.0,
                    api::ChannelType::General,
                ),
                pr_channel(
                    dt("2020-01-01T00:10:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    9.0,
                    api::ChannelType::General,
                ),
            ],
        );
        let current = overlapping.current(&dt("2020-01-01T00:15:00Z"));
        assert_eq!(current.len(), 1);
        assert_approx_eq!(f32, current[0].per_kwh, 1.0);

        // Nothing is current outside all intervals.
        assert_eq!(
            prices.current(&dt("2020-01-01T02:00:00Z")),
            [] as [&api::PriceResponse; 0]
        );
    }

    #[test]
    fn test_combined_prices_equality() {
        let id = Id::new("test").unwrap();

        let prices_fn = || {
            vec![
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    1.0,
                    api::ChannelType::General,
                ),
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    2.0,
                    api::ChannelType::ControlledLoad,
                ),
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    3.0,
                    api::ChannelType::FeedIn,
                ),
            ]
        };

        // Equal content in separately allocated instances is equal, even
        // when the input interleaves channel types differently.
        let a = CombinedPrices(Arc::new(Prices::new(&id, prices_fn())));
        let mut reversed = prices_fn();
        reversed.reverse();
        let b = CombinedPrices(Arc::new(Prices::new(&id, reversed)));
        assert_eq!(a, b);

        // Different content is not equal.
        let mut other = prices_fn();
        other[0].per_kwh = 100.0;
        let c = CombinedPrices(Arc::new(Prices::new(&id, other)));
        assert_ne!(a, c);
    }

    #[test]
    fn test_combined_prices_serialize() {
        let id = Id::new("test").unwrap();

        // Input interleaves channel types.
        let prices = vec![
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                1.0,
                api::ChannelType::General,
            ),
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                2.0,
                api::ChannelType::ControlledLoad,
            ),
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                3.0,
                api::ChannelType::FeedIn,
            ),
            pr_channel(
                dt("2020-01-01T00:30:00Z"),
                dt("2020-01-01T01:00:00Z"),
                4.0,
                api::ChannelType::General,
            ),
        ];
        let combined_prices = CombinedPrices(Arc::new(Prices::new(&id, prices)));

        // The MQTT payload is a single list that deserializes back into the
        // combined order (the wire contract the frontend depends on when it
        // parses the prices topic as a Vec<PriceResponse>).
        let json = serde_json::to_value(&combined_prices).unwrap();
        let deserialized: Vec<api::PriceResponse> = serde_json::from_value(json).unwrap();
        let expected = vec![
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                1.0,
                api::ChannelType::General,
            ),
            pr_channel(
                dt("2020-01-01T00:30:00Z"),
                dt("2020-01-01T01:00:00Z"),
                4.0,
                api::ChannelType::General,
            ),
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                2.0,
                api::ChannelType::ControlledLoad,
            ),
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                3.0,
                api::ChannelType::FeedIn,
            ),
        ];
        assert_eq!(deserialized, expected);
    }
}
