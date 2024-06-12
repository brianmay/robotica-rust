use super::Prices;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tap::Pipe;
use tracing::error;

#[derive(Copy, Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum PriceCategory {
    SuperCheap,
    Cheap,
    Normal,
    Expensive,
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
    prices: &Prices,
    dt: DateTime<Utc>,
) -> Option<PriceCategory> {
    prices.get_weighted_price(dt).map_or_else(
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
    use super::PriceCategory;
    use super::*;

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
}
