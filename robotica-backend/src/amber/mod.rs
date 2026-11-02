use std::sync::Arc;

use chrono::{DateTime, FixedOffset, TimeDelta, Timelike, Utc};
use robotica_common::datetime::utc_now;
use robotica_common::robotica::entities::Id;
use robotica_macro::time_delta_constant;
use robotica_tokio::{
    pipes::{
        stateful::{create_pipe, Receiver},
        stateless,
    },
    spawn,
};
use thiserror::Error;
use tokio::time::{interval, sleep_until, Instant, MissedTickBehavior};
use tracing::info;
use tracing_log_error::log_error;

pub use robotica_common::robotica::amber::price::{GeneralPrices, Prices};

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
                            let prices = Arc::new(Prices::new(&id, now, prices));
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

    use super::*;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
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
    fn test_get_current_period_end_with_current_period() {
        let id = Id::new("test").unwrap();

        let prices = Prices::new(
            &id,
            dt("2020-01-01T00:00:00Z"),
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
            dt("2020-01-01T00:00:00Z"),
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
}
