use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tap::Pipe;
use tracing::{error, info};

use super::api;

#[derive(Copy, Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum PriceCategory {
    SuperCheap,
    Cheap,
    Normal,
    Expensive,
}

fn get_weighted_price(prices: &[api::PriceResponse], dt: &DateTime<Utc>) -> Option<f32> {
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

#[allow(clippy::module_name_repetitions)]
pub fn get_weighted_price_category(
    prices: &[api::PriceResponse],
    dt: &DateTime<Utc>,
    old_category: Option<PriceCategory>,
) -> Option<PriceCategory> {
    get_weighted_price(prices, dt).map_or_else(
        || {
            error!("Get weighted price found in failed: {prices:?}");
            None
        },
        |weighted_price| get_price_category(old_category, weighted_price).pipe(Some),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]
    use super::{api::IntervalType, PriceCategory};
    use chrono::Local;
    use float_cmp::assert_approx_eq;
    use test_log::test;

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
        let p = get_weighted_price(&prices, &now).unwrap();
        assert_approx_eq!(f32, p, 1.25);

        let now = dt("2020-01-01T00:30:00Z");
        let prices = prices_fn(1);
        let p = get_weighted_price(&prices, &now).unwrap();
        assert_approx_eq!(f32, p, 2.25);

        let now = dt("2020-01-01T01:00:00Z");
        let prices = prices_fn(2);
        let p = get_weighted_price(&prices, &now).unwrap();
        assert_approx_eq!(f32, p, 3.5);

        let now = dt("2020-01-01T01:30:00Z");
        let prices = prices_fn(3);
        let p = get_weighted_price(&prices, &now);
        assert!(p.is_none());
    }
}
