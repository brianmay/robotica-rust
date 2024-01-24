use super::{PriceCategory, Prices};
use chrono::{DateTime, NaiveTime, TimeZone, Utc};
use robotica_backend::{
    pipes::{
        stateful::{create_pipe, Receiver},
        Subscriber, Subscription,
    },
    spawn,
};
use std::sync::Arc;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ChargeRequest {
    ChargeTo(u8),
    //DoNotCharge,
}

pub fn run(rx: Receiver<Arc<Prices>>) -> Receiver<ChargeRequest> {
    let (tx_out, rx_out) = create_pipe("amber/car");

    spawn(async move {
        let mut s = rx.subscribe().await;

        while let Ok(prices) = s.recv().await {
            let cr = prices_to_charge_request(&prices, Utc::now(), &Utc);
            tx_out.try_send(cr);
        }
    });

    rx_out
}

fn prices_to_charge_request<T: TimeZone>(
    prices: &Prices,
    dt: DateTime<Utc>,
    tz: &T,
) -> ChargeRequest {
    let now = dt.with_timezone(tz);
    let time = now.time();

    #[allow(clippy::unwrap_used)]
    let start_time = NaiveTime::from_hms_opt(3, 0, 0).unwrap();
    #[allow(clippy::unwrap_used)]
    let end_time = NaiveTime::from_hms_opt(6, 30, 0).unwrap();
    let force = time > start_time && time < end_time;

    #[allow(clippy::match_same_arms)]
    match (force, prices.category) {
        (_, PriceCategory::SuperCheap) => ChargeRequest::ChargeTo(90),
        (_, PriceCategory::Cheap) => ChargeRequest::ChargeTo(80),
        (true, PriceCategory::Normal) => ChargeRequest::ChargeTo(70),
        (false, PriceCategory::Normal) => ChargeRequest::ChargeTo(50),
        (true, PriceCategory::Expensive) => ChargeRequest::ChargeTo(50),
        (false, PriceCategory::Expensive) => ChargeRequest::ChargeTo(20),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]

    use super::*;

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
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(90));

        let now = dt("2020-01-01T00:00:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::Cheap,
            dt: now,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(80));

        let now = dt("2020-01-01T00:00:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::Normal,
            dt: now,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(50));

        let now = dt("2020-01-01T00:00:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::Expensive,
            dt: now,
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
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(90));

        let now = dt("2020-01-01T03:30:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::Cheap,
            dt: now,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(80));

        let now = dt("2020-01-01T03:30:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::Normal,
            dt: now,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(70));

        let now = dt("2020-01-01T03:30:00Z");
        let summary = Prices {
            list: vec![],
            category: PriceCategory::Expensive,
            dt: now,
        };
        let cr = prices_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(50));
    }
}
