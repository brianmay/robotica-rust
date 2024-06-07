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

fn get_price_category(is_charging: bool, price: f32) -> PriceCategory {
    if is_charging {
        match price {
            x if x < 11.0 => PriceCategory::SuperCheap,
            x if x < 16.0 => PriceCategory::Cheap,
            x if x < 31.0 => PriceCategory::Normal,
            _ => PriceCategory::Expensive,
        }
    } else {
        match price {
            x if x < 9.0 => PriceCategory::SuperCheap,
            x if x < 14.0 => PriceCategory::Cheap,
            x if x < 29.0 => PriceCategory::Normal,
            _ => PriceCategory::Expensive,
        }
    }
}

#[allow(clippy::module_name_repetitions)]
pub fn get_weighted_price_category(
    is_charging: bool,
    prices: &[api::PriceResponse],
    dt: &DateTime<Utc>,
) -> Option<PriceCategory> {
    get_weighted_price(prices, dt).map_or_else(
        || {
            error!("Get weighted price found in failed: {prices:?}");
            None
        },
        |weighted_price| get_price_category(is_charging, weighted_price).pipe(Some),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]
    use super::*;
    use super::{api::IntervalType, PriceCategory};
    use chrono::Local;
    use float_cmp::assert_approx_eq;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    #[rstest::rstest]
    #[case(false, 8.9, PriceCategory::SuperCheap)]
    #[case(false, 9.0, PriceCategory::Cheap)]
    #[case(false, 9.1, PriceCategory::Cheap)]
    #[case(false, 13.9, PriceCategory::Cheap)]
    #[case(false, 14.0, PriceCategory::Normal)]
    #[case(false, 14.1, PriceCategory::Normal)]
    #[case(false, 28.9, PriceCategory::Normal)]
    #[case(false, 29.0, PriceCategory::Expensive)]
    #[case(false, 29.1, PriceCategory::Expensive)]
    #[case(true, 10.9, PriceCategory::SuperCheap)]
    #[case(true, 11.0, PriceCategory::Cheap)]
    #[case(true, 11.1, PriceCategory::Cheap)]
    #[case(true, 15.9, PriceCategory::Cheap)]
    #[case(true, 16.0, PriceCategory::Normal)]
    #[case(true, 16.1, PriceCategory::Normal)]
    #[case(true, 30.9, PriceCategory::Normal)]
    #[case(true, 31.0, PriceCategory::Expensive)]
    #[case(true, 31.1, PriceCategory::Expensive)]

    fn test_get_price_category(
        #[case] is_charging: bool,
        #[case] price: f32,
        #[case] expected: PriceCategory,
    ) {
        let c = get_price_category(is_charging, price);
        assert_eq!(
            c, expected,
            "get_price_category({is_charging:?}, {price:?}) = {c:?} != {expected:?}"
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
