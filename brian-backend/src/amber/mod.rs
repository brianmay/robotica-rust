use std::sync::Arc;

use chrono::{DateTime, Duration, FixedOffset, Utc};
use robotica_backend::{
    pipes::stateful::{create_pipe, Receiver},
    spawn,
};
use robotica_common::datetime::utc_now;
use tap::Pipe;
use thiserror::Error;
use tokio::time::{interval, sleep_until, Instant, MissedTickBehavior};
use tracing::{error, info};

pub mod api;
pub mod car;
pub mod logging;

#[derive(Copy, Debug, Clone, Eq, PartialEq)]
pub enum PriceCategory {
    SuperCheap,
    Cheap,
    Normal,
    Expensive,
}

pub struct Prices {
    pub list: Vec<api::PriceResponse>,
    pub category: PriceCategory,
    pub dt: DateTime<Utc>,
}

impl PartialEq for Prices {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

impl Eq for Prices {}

impl Prices {
    pub fn current(&self) -> Option<&api::PriceResponse> {
        get_current_price_response(&self.list, &self.dt)
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

        // Assume normal price category until proven otherwise
        let mut category = None;

        loop {
            tokio::select! {
                () = sleep_until(price_instant) => {
                    let now = utc_now();
                    let today = now.with_timezone(&nem_timezone).date_naive();
                    let yesterday = today - Duration::days(1);
                    let tomorrow = today + Duration::days(1);

                    // Get prices for the current interval.
                    let prices = api::get_prices(&config, yesterday, tomorrow).await;

                    // Process the results.
                    let next_delay = match prices {
                        Ok(prices) => {
                            let update_time = get_current_price_response(&prices, &now).map_or_else(|| {
                                error!("No current price found in prices: {prices:?}");
                                // If we failed to get a current price, try again in 1 minute
                                now + Duration::minutes(1)
                            }, |current_price| {
                                info!("Current price: {current_price:?}");
                                current_price.end_time
                            });

                            get_weighted_price(&prices, &now).map_or_else(|| {
                                error!("Get weighted price found in failed: {prices:?}");
                            }, |weighted_price| {
                                let c = get_price_category(category, weighted_price);
                                category = Some(c);
                            });

                            info!("Price category: {category:?}");
                            if let Some(category) = category {
                                tx_prices.try_send(Arc::new(Prices {
                                    list: prices,
                                    category,
                                    dt: now,
                                }));
                            }

                            {
                                // How long to the current interval expires?
                                let now = utc_now();
                                let duration: Duration = update_time - now;
                                info!("Next price update: {update_time:?} in {duration}");

                                // Ensure we update prices at least once once every 5 minutes.
                                let max_duration = Duration::minutes(5);
                                let min_duration = Duration::seconds(30);
                                duration.clamp(min_duration, max_duration)
                            }
                        }
                        Err(err) => {
                            error!("Failed to get prices: {}", err);
                            // If we failed to get prices, try again in 1 minute
                            Duration::minutes(1)
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
                    let yesterday = today - Duration::days(1);
                    let tomorrow = today + Duration::days(1);

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

fn get_weighted_price(prices: &[api::PriceResponse], dt: &DateTime<Utc>) -> Option<f32> {
    let pos = prices.iter().position(|pr| is_period_current(pr, dt));

    let Some(pos) = pos else {
        return None;
    };

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

fn get_price_category(category: Option<PriceCategory>, price: f32) -> PriceCategory {
    let mut c = category.unwrap_or(PriceCategory::Normal);

    let under = |c: PriceCategory, threshold: f32, new_category: PriceCategory| {
        // If all prices are under the threshold, then change the category.
        if price < threshold {
            new_category
        } else {
            c
        }
    };
    let over = |c: PriceCategory, threshold: f32, new_category: PriceCategory| {
        // If the current price is over the threshold, then change the category.
        if price > threshold {
            new_category
        } else {
            c
        }
    };

    match c {
        PriceCategory::SuperCheap => {
            c = over(c, 11.0, PriceCategory::Cheap);
            c = over(c, 16.0, PriceCategory::Normal);
            c = over(c, 31.0, PriceCategory::Expensive);
        }
        PriceCategory::Cheap => {
            c = over(c, 16.0, PriceCategory::Normal);
            c = over(c, 31.0, PriceCategory::Expensive);
            c = under(c, 9.0, PriceCategory::SuperCheap);
        }
        PriceCategory::Normal => {
            c = over(c, 31.0, PriceCategory::Expensive);
            c = under(c, 14.0, PriceCategory::Cheap);
            c = under(c, 9.0, PriceCategory::SuperCheap);
        }
        PriceCategory::Expensive => {
            c = under(c, 29.0, PriceCategory::Normal);
            c = under(c, 14.0, PriceCategory::Cheap);
            c = under(c, 9.0, PriceCategory::SuperCheap);
        }
    }

    c
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]
    use chrono::Local;
    use float_cmp::assert_approx_eq;

    use super::*;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    #[test]
    fn test_get_price_category() {
        use PriceCategory::{Cheap, Expensive, Normal, SuperCheap};

        // For < thresholds, all prices must be < threshold
        // For > thresholds, only current price must be > threshold
        let data = [
            // Super cheap thresholds >11.0 Cheap >16.0 Normal >31.0 Expensive
            (SuperCheap, 10.0, SuperCheap),
            (SuperCheap, 11.1, Cheap),
            (SuperCheap, 16.0, Cheap),
            (SuperCheap, 16.1, Normal),
            (SuperCheap, 31.0, Normal),
            (SuperCheap, 31.1, Expensive),
            // Cheap thresholds >16.0 Normal >31.0 Expensive <9.0 SuperCheap
            (Cheap, 8.9, SuperCheap),
            (Cheap, 9.0, Cheap),
            (Cheap, 11.1, Cheap),
            (Cheap, 16.0, Cheap),
            (Cheap, 16.1, Normal),
            (Cheap, 31.0, Normal),
            (Cheap, 31.1, Expensive),
            // Normal thresholds >31.0 Expensive <14.0 Cheap <9.0 SuperCheap
            (Normal, 8.9, SuperCheap),
            (Normal, 9.0, Cheap),
            (Normal, 13.9, Cheap),
            (Normal, 14.0, Normal),
            (Normal, 31.0, Normal),
            (Normal, 31.1, Expensive),
            // Expensive thresholds <29.0 Normal <14.0 Cheap <9.0 SuperCheap
            (Expensive, 8.9, SuperCheap),
            (Expensive, 9.0, Cheap),
            (Expensive, 13.9, Cheap),
            (Expensive, 14.0, Normal),
            (Expensive, 28.9, Normal),
            (Expensive, 29.0, Expensive),
        ];

        for d in data {
            let c = get_price_category(Some(d.0), d.1);
            assert_eq!(c, d.2, "get_price_category({:?}, {:?}) = {:?}", d.0, d.1, c);
        }
    }

    #[test]
    fn test_is_period_current() {
        let pr = |start_time: DateTime<Utc>, end_time: DateTime<Utc>| api::PriceResponse {
            date: start_time.with_timezone(&Local).date_naive(),
            start_time,
            end_time,
            per_kwh: 0.0,
            spot_per_kwh: 0.0,
            interval_type: api::IntervalType::CurrentInterval,
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

        let now = dt("2020-01-01T00:00:00Z");
        let p = pr(dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z"));
        assert_eq!(is_period_current(&p, &now), true);

        let p = pr(dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:00:00Z"));
        assert_eq!(is_period_current(&p, &now), false);

        let p = pr(dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:00:01Z"));
        assert_eq!(is_period_current(&p, &now), true);

        let p = pr(dt("2019-01-01T23:59:59Z"), dt("2020-01-01T00:00:00Z"));
        assert_eq!(is_period_current(&p, &now), false);

        let p = pr(dt("2019-01-01T23:59:59Z"), dt("2020-01-01T00:00:01Z"));
        assert_eq!(is_period_current(&p, &now), true);
    }

    #[test]
    fn test_get_current_price_response() {
        let pr = |start_time: DateTime<Utc>, end_time: DateTime<Utc>| api::PriceResponse {
            date: start_time.with_timezone(&Local).date_naive(),
            start_time,
            end_time,
            per_kwh: 0.0,
            spot_per_kwh: 0.0,
            interval_type: api::IntervalType::CurrentInterval,
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

        let prices = vec![
            pr(dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z")),
            pr(dt("2020-01-01T00:30:00Z"), dt("2020-01-01T01:00:00Z")),
            pr(dt("2020-01-01T01:00:00Z"), dt("2020-01-01T01:30:00Z")),
        ];

        let now = dt("2019-12-31T23:59:59Z");
        let p = get_current_price_response(&prices, &now);
        assert!(p.is_none());

        let now = dt("2020-01-01T00:00:00Z");
        let p = get_current_price_response(&prices, &now).unwrap();
        assert_eq!(p.start_time, prices[0].start_time);
        assert_eq!(p.end_time, prices[0].end_time);

        let now = dt("2020-01-01T00:30:00Z");
        let p = get_current_price_response(&prices, &now).unwrap();
        assert_eq!(p.start_time, prices[1].start_time);
        assert_eq!(p.end_time, prices[1].end_time);

        let now = dt("2020-01-01T01:00:00Z");
        let p = get_current_price_response(&prices, &now).unwrap();
        assert_eq!(p.start_time, prices[2].start_time);
        assert_eq!(p.end_time, prices[2].end_time);

        let now = dt("2020-01-01T01:30:00Z");
        let p = get_current_price_response(&prices, &now);
        assert!(p.is_none());
    }

    #[test]
    fn test_get_weighted_price() {
        let pr = |start_time: DateTime<Utc>, end_time: DateTime<Utc>, price| api::PriceResponse {
            date: start_time.with_timezone(&Local).date_naive(),
            start_time,
            end_time,
            per_kwh: price,
            spot_per_kwh: price,
            interval_type: api::IntervalType::CurrentInterval,
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

        let prices = vec![
            pr(dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z"), 1.0),
            pr(dt("2020-01-01T00:30:00Z"), dt("2020-01-01T01:00:00Z"), 2.0),
            pr(dt("2020-01-01T01:00:00Z"), dt("2020-01-01T01:30:00Z"), 4.0),
        ];

        let now = dt("2020-01-01T00:00:00Z");
        let p = get_weighted_price(&prices, &now).unwrap();
        assert_approx_eq!(f32, p, 1.25);

        let now = dt("2020-01-01T00:30:00Z");
        let p = get_weighted_price(&prices, &now).unwrap();
        assert_approx_eq!(f32, p, 2.25);

        let now = dt("2020-01-01T01:00:00Z");
        let p = get_weighted_price(&prices, &now).unwrap();
        assert_approx_eq!(f32, p, 3.5);

        let now = dt("2020-01-01T01:30:00Z");
        let p = get_weighted_price(&prices, &now);
        assert!(p.is_none());
    }
}
