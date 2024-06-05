use crate::delays::rate_limit;

use super::{PriceCategory, Prices};
use chrono::{DateTime, Local, NaiveTime, TimeZone, Utc};
use robotica_backend::{
    pipes::{
        stateful::{create_pipe, Receiver},
        Subscriber, Subscription,
    },
    spawn,
};
use robotica_common::unsafe_naive_time_hms;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ChargeRequest {
    ChargeTo(u8),
    //DoNotCharge,
    Manual,
}

impl ChargeRequest {
    pub const fn is_auto(self) -> bool {
        match self {
            Self::ChargeTo(_) => true,
            Self::Manual => false,
        }
    }
}

pub fn run(rx: Receiver<Arc<Prices>>) -> Receiver<ChargeRequest> {
    let (tx_out, rx_out) = create_pipe("amber/car");

    spawn(async move {
        let mut s = rx.subscribe().await;

        while let Ok(prices) = s.recv().await {
            let cr = prices_to_charge_request(&prices, Utc::now(), &Local);
            tx_out.try_send(cr);
        }
    });

    rate_limit("amber/car/ratelimit", Duration::from_secs(300), rx_out)
}

const START_TIME: NaiveTime = unsafe_naive_time_hms!(3, 0, 0);
const END_TIME: NaiveTime = unsafe_naive_time_hms!(6, 30, 0);

fn prices_to_charge_request<T: TimeZone>(
    prices: &Prices,
    dt: DateTime<Utc>,
    tz: &T,
) -> ChargeRequest {
    let now = dt.with_timezone(tz);
    let time = now.time();

    let force = time >= START_TIME && time < END_TIME;

    #[allow(clippy::match_same_arms)]
    let result = match (force, prices.category) {
        (_, PriceCategory::SuperCheap) => ChargeRequest::ChargeTo(90),
        (_, PriceCategory::Cheap) => ChargeRequest::ChargeTo(80),
        (true, PriceCategory::Normal) => ChargeRequest::ChargeTo(70),
        (false, PriceCategory::Normal) => ChargeRequest::ChargeTo(50),
        (true, PriceCategory::Expensive) => ChargeRequest::ChargeTo(50),
        (false, PriceCategory::Expensive) => ChargeRequest::ChargeTo(20),
    };

    info!(
        "Charge request ({time:?},{force:?},{category:?}): {result:?}",
        category = prices.category
    );
    result
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]

    use std::time::Duration;

    use robotica_common::unsafe_duration;

    use super::*;

    const INTERVAL: Duration = unsafe_duration!(minutes: 30);

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    #[test]
    fn test_prices_to_charge_request_normal() {
        let now = dt("2020-01-01T00:00:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::SuperCheap,
            dt: now,
            interval: INTERVAL,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(90));

        let now = dt("2020-01-01T00:00:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::Cheap,
            dt: now,
            interval: INTERVAL,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(80));

        let now = dt("2020-01-01T00:00:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::Normal,
            dt: now,
            interval: INTERVAL,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(50));

        let now = dt("2020-01-01T00:00:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::Expensive,
            dt: now,
            interval: INTERVAL,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(20));
    }

    #[test]
    fn test_summary_to_charge_request_forced() {
        let now = dt("2020-01-01T03:30:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::SuperCheap,
            dt: now,
            interval: INTERVAL,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(90));

        let now = dt("2020-01-01T03:30:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::Cheap,
            dt: now,
            interval: INTERVAL,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(80));

        let now = dt("2020-01-01T03:30:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::Normal,
            dt: now,
            interval: INTERVAL,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(70));

        let now = dt("2020-01-01T03:30:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::Expensive,
            dt: now,
            interval: INTERVAL,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(50));
    }
}
