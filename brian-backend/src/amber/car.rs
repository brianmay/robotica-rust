use crate::{delays::rate_limit, tesla::TeslamateId, InitState};

use super::{
    price_category::{get_weighted_price_category, PriceCategory},
    Prices,
};
use chrono::{DateTime, Local, NaiveTime, TimeDelta, TimeZone, Utc};
use robotica_backend::{
    pipes::{
        stateful::{self, create_pipe, Receiver},
        stateless, Subscriber, Subscription,
    },
    spawn,
};
use robotica_common::{
    datetime::time_delta,
    mqtt::{MqttMessage, Parsed, QoS, Retain},
    unsafe_naive_time_hms,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tracing::{debug, error, info};

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize)]
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

    pub fn max(self, other: Self) -> Self {
        match (self, other) {
            (Self::ChargeTo(a), Self::ChargeTo(b)) => Self::ChargeTo(a.max(b)),
            (Self::ChargeTo(a), Self::Manual) => Self::ChargeTo(a),
            (Self::Manual, Self::ChargeTo(b)) => Self::ChargeTo(b),
            (Self::Manual, Self::Manual) => Self::Manual,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistentState {
    min_charge_tomorrow: u8,
}

impl Default for PersistentState {
    fn default() -> Self {
        Self {
            min_charge_tomorrow: 70,
        }
    }
}

pub fn run(
    state: &InitState,
    teslamate_id: TeslamateId,
    rx: Receiver<Arc<Prices>>,
    battery_level: stateful::Receiver<Parsed<u8>>,
    min_charge_tomorrow: stateless::Receiver<Parsed<u8>>,
    is_charging: stateful::Receiver<bool>,
) -> Receiver<ChargeRequest> {
    let (tx_out, rx_out) = create_pipe("amber/car");
    let id = teslamate_id.to_string();
    let mqtt = state.mqtt.clone();

    let psr = state
        .persistent_state_database
        .for_name::<PersistentState>(&format!("tesla_amber_{id}"));
    let mut ps = psr.load().unwrap_or_default();

    spawn(async move {
        let mut s = rx.subscribe().await;
        let mut s_min_charge_tomorrow = min_charge_tomorrow.subscribe().await;
        let mut s_battery_level = battery_level.subscribe().await;
        let mut s_is_charging = is_charging.subscribe().await;

        let Ok(mut v_prices) = s.recv().await else {
            error!("{id}: Failed to get initial prices");
            return;
        };
        let Ok(mut v_battery_level) = s_battery_level.recv().await.as_deref().copied() else {
            error!("{id}: Failed to get initial battery level");
            return;
        };
        let Ok(mut v_is_charging) = s_is_charging.recv().await else {
            error!("{id}: Failed to get initial charging state");
            return;
        };

        save_state(teslamate_id, &psr, &ps);

        loop {
            info!("{id}: Persistent State: {:?}", ps);
            let cr = prices_to_charge_request(
                teslamate_id,
                &v_prices,
                v_battery_level,
                v_is_charging,
                &ps,
                Utc::now(),
                &Local,
            );

            info!("{id}: Charging request: {:#?}", cr);
            publish_state(teslamate_id, &cr, &mqtt);
            tx_out.try_send(cr.result);

            select! {
                Ok(prices) = s.recv() => {
                    v_prices = prices;
                },
                Ok(battery_level) = s_battery_level.recv() => {
                    v_battery_level = *battery_level;
                },
                Ok(is_charging) = s_is_charging.recv() => {
                    v_is_charging = is_charging;
                },
                Ok(min_charge_tomorrow) = s_min_charge_tomorrow.recv() => {
                    debug!("{id}: Setting min charge tomorrow to {}", *min_charge_tomorrow);
                    ps.min_charge_tomorrow = *min_charge_tomorrow;
                    save_state(teslamate_id, &psr, &ps);

                },
                else => break,
            }
        }
    });

    rate_limit("amber/car/ratelimit", Duration::from_secs(300), rx_out)
}

fn save_state(
    teslamate_id: TeslamateId,
    psr: &robotica_backend::services::persistent_state::PersistentStateRow<PersistentState>,
    ps: &PersistentState,
) {
    psr.save(ps).unwrap_or_else(|e| {
        let id = teslamate_id.to_string();
        error!("{id}: Failed to save persistent state: {:?}", e);
    });
}

const END_TIME: NaiveTime = unsafe_naive_time_hms!(6, 30, 0);

fn prices_to_charge_request<T: TimeZone>(
    teslamate_id: TeslamateId,
    prices: &Prices,
    battery_level: u8,
    is_charging: bool,
    ps: &PersistentState,
    dt: DateTime<Utc>,
    tz: &T,
) -> State {
    let id = teslamate_id.to_string();

    // How long should car take to charge to min_charge_tomorrow?
    let estimated_charge_time_to_min =
        estimate_to_limit(teslamate_id, battery_level, dt, ps.min_charge_tomorrow, tz);

    // Get the cheapest price for the estimated charge time
    let cheapest_price = estimated_charge_time_to_min.and_then(|estimated_charge_time_to_min| {
        let (_start_time, end_time) = super::private::get_day(&dt, END_TIME, tz);
        prices.get_cheapest_price_for_time_delta(estimated_charge_time_to_min, &dt, &end_time)
    });

    // If car is charging we raise the threshold for the cheapest price.
    // To try to prevent cycling with fluctuating prices.
    let threshold_price = match (is_charging, cheapest_price) {
        (true, Some(cheapest_price)) => Some(cheapest_price * 1.1),
        (false, Some(cheapest_price)) => Some(cheapest_price),
        _ => None,
    };

    // What is the current price?
    let current_price = prices.current(&dt).map(|p| p.per_kwh);
    info!(
        "{id}: Is charging: {is_charging}, cheapest price is {cheapest_price:?}, threshold price is {threshold_price:?}, current price is {current_price:?}",
    );

    // Should we force a charge?
    let force = match (threshold_price, current_price) {
        (Some(cheapest_price), Some(current_price)) if current_price <= cheapest_price => {
            Some(ChargeRequest::ChargeTo(ps.min_charge_tomorrow))
        }
        _ => None,
    };

    // Get the normal charge request based on category
    let category = get_weighted_price_category(is_charging, &prices.list, &dt);
    #[allow(clippy::match_same_arms)]
    let normal = match category {
        Some(PriceCategory::SuperCheap) => ChargeRequest::ChargeTo(90),
        Some(PriceCategory::Cheap) => ChargeRequest::ChargeTo(80),
        Some(PriceCategory::Normal) => ChargeRequest::ChargeTo(50),
        Some(PriceCategory::Expensive) => ChargeRequest::ChargeTo(20),
        None => ChargeRequest::ChargeTo(50),
    };
    info!(
        "{id}: Price Category: {category:?}, Normal charge request: {normal:?}",
        category = category,
        normal = normal
    );

    // get the largest value out of force and normal
    let result = force.map_or(normal, |force| {
        let result = normal.max(force);
        info!("{id}: Forcing charge to {force:?}, now {result:?}");
        result
    });

    // Get some more stats
    let estimated_charge_time_to_limit = match result {
        ChargeRequest::ChargeTo(limit) => {
            estimate_to_limit(teslamate_id, battery_level, dt, limit, tz)
        }
        ChargeRequest::Manual => None,
    };
    let estimated_charge_time_to_90 = estimate_to_limit(teslamate_id, battery_level, dt, 90, tz);

    State {
        time: dt,
        battery_level,
        min_charge_tomorrow: ps.min_charge_tomorrow,
        cheapest_price,
        current_price,
        force,
        normal,
        result,
        estimated_charge_time_to_min,
        estimated_charge_time_to_limit,
        estimated_charge_time_to_90,
    }
}

fn estimate_to_limit<T: TimeZone>(
    teslamate_id: TeslamateId,
    battery_level: u8,
    dt: DateTime<Utc>,
    limit: u8,
    tz: &T,
) -> Option<TimeDelta> {
    let estimated_charge_time = estimate_charge_time(battery_level, limit);
    let id = teslamate_id.to_string();
    if let Some(estimated_charge_time) = estimated_charge_time {
        let estimated_finish = dt + estimated_charge_time;
        info!(
            "{id}: Estimated charge time to {limit} is {time}, should finish at {finish:?}",
            id = id,
            time = time_delta::to_string(&estimated_charge_time),
            finish = estimated_finish.with_timezone(tz).to_rfc3339()
        );
    } else {
        info!(
            "{id}: Battery level is already at or above {limit}",
            limit = limit
        );
    }
    estimated_charge_time
}

#[derive(Debug, Serialize, PartialEq)]
struct State {
    time: DateTime<Utc>,
    battery_level: u8,
    min_charge_tomorrow: u8,
    cheapest_price: Option<f32>,
    current_price: Option<f32>,
    force: Option<ChargeRequest>,
    normal: ChargeRequest,
    result: ChargeRequest,

    #[serde(with = "robotica_common::datetime::with_option_time_delta")]
    estimated_charge_time_to_min: Option<TimeDelta>,

    #[serde(with = "robotica_common::datetime::with_option_time_delta")]
    estimated_charge_time_to_limit: Option<TimeDelta>,

    #[serde(with = "robotica_common::datetime::with_option_time_delta")]
    estimated_charge_time_to_90: Option<TimeDelta>,
}

fn publish_state(
    teslamate_id: TeslamateId,
    state: &State,
    mqtt: &robotica_backend::services::mqtt::MqttTx,
) {
    let topic = format!("state/Tesla/{id}/Amber", id = teslamate_id.to_string());
    let result = MqttMessage::from_json(topic, &state, Retain::Retain, QoS::AtLeastOnce);
    match result {
        Ok(msg) => mqtt.try_send(msg),
        Err(e) => error!("Failed to serialize state: {:?}", e),
    }
}

const fn estimate_charge_time(battery_level: u8, min_charge_tomorrow: u8) -> Option<TimeDelta> {
    let min_charge_tomorrow = min_charge_tomorrow as i64;
    let battery_level = battery_level as i64;

    let diff = min_charge_tomorrow - battery_level;
    if diff <= 0 {
        None
    } else {
        let charge_time = diff * 280 / 39;
        Some(TimeDelta::minutes(charge_time))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]

    use crate::amber::api::{self, IntervalType};

    use super::*;
    use robotica_common::unsafe_duration;
    use std::time::Duration;

    const INTERVAL: Duration = unsafe_duration!(minutes: 30);

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

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

    fn pr_list_fixed(cost: f32) -> Vec<api::PriceResponse> {
        let time = dt("2020-01-01T00:00:00Z");

        (0i8..48i8)
            .map(|i| {
                let i64 = i64::from(i);
                pr(
                    time + TimeDelta::minutes(i64 * 30),
                    time + TimeDelta::minutes((i64 + 1) * 30),
                    IntervalType::ForecastInterval,
                    cost,
                )
            })
            .collect::<Vec<api::PriceResponse>>()
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

    #[test_log::test(rstest::rstest)]
    #[case(5.0, ChargeRequest::ChargeTo(90))]
    #[case(12.5, ChargeRequest::ChargeTo(80))]
    #[case(20.0, ChargeRequest::ChargeTo(50))]
    #[case(35.0, ChargeRequest::ChargeTo(20))]
    fn test_prices_to_charge_request_normal(#[case] price: f32, #[case] expected: ChargeRequest) {
        let now = dt("2020-01-01T00:00:30Z");
        let summary = Prices {
            list: pr_list_fixed(price),
            dt: now,
            interval: INTERVAL,
        };
        let ps = PersistentState {
            min_charge_tomorrow: 0,
        };
        let battery_level = 70u8;
        let cr = prices_to_charge_request(
            TeslamateId::testing_value(),
            &summary,
            battery_level,
            false,
            &ps,
            now,
            &Utc,
        );
        assert_eq!(cr.time, now);
        assert_eq!(cr.battery_level, battery_level);
        assert_eq!(cr.min_charge_tomorrow, 0);
        assert_eq!(cr.cheapest_price, None);
        assert_eq!(cr.current_price, Some(price));
        assert_eq!(cr.force, None);
        assert_eq!(cr.normal, expected);
        assert_eq!(cr.result, expected);
    }

    #[test_log::test(rstest::rstest)]
    #[case(dt("2020-01-01T03:30:30Z"), Some(46.5), Some(46.5), true)]
    #[case(dt("2020-01-01T06:00:30Z"), Some(44.0), Some(44.0), true)]
    #[case(dt("2020-01-01T06:30:30Z"), Some(33.5), Some(43.5), false)]
    #[case(dt("2020-01-01T07:00:30Z"), Some(33.5), Some(43.0), false)]
    fn test_prices_to_charge_request_forced(
        #[case] now: DateTime<Utc>,
        #[case] expected_price: Option<f32>,
        #[case] expected_current_price: Option<f32>,
        #[case] forced: bool,
    ) {
        let summary = Prices {
            // SuperCheap
            list: pr_list_descending(50.0),
            dt: now,
            interval: INTERVAL,
        };
        let ps = PersistentState {
            min_charge_tomorrow: 72,
        };
        let battery_level = 10u8;
        let cr = prices_to_charge_request(
            TeslamateId::testing_value(),
            &summary,
            battery_level,
            false,
            &ps,
            now,
            &Utc,
        );
        assert_eq!(cr.time, now);
        assert_eq!(cr.battery_level, battery_level);
        assert_eq!(cr.min_charge_tomorrow, 72);
        assert_eq!(cr.cheapest_price, expected_price);
        assert_eq!(cr.current_price, expected_current_price);
        if forced {
            assert_eq!(cr.force, Some(ChargeRequest::ChargeTo(72)));
            assert_eq!(cr.result, ChargeRequest::ChargeTo(72));
        } else {
            assert_eq!(cr.force, None);
            assert_eq!(cr.result, ChargeRequest::ChargeTo(20));
        }
        assert_eq!(cr.normal, ChargeRequest::ChargeTo(20));
    }

    #[test_log::test(rstest::rstest)]
    #[case(dt("2020-01-01T03:30:30Z"), Some(-3.5), Some(-3.5), true)]
    #[case(dt("2020-01-01T06:00:30Z"), Some(-6.0), Some(-6.0), true)]
    #[case(dt("2020-01-01T06:30:30Z"), Some(-16.5), Some(-6.5), false)]
    #[case(dt("2020-01-01T07:00:30Z"), Some(-16.5), Some(-7.0), false)]
    fn test_prices_to_charge_request_combined_forced_and_cheap(
        #[case] now: DateTime<Utc>,
        #[case] expected_price: Option<f32>,
        #[case] expected_current_price: Option<f32>,
        #[case] forced: bool,
    ) {
        let summary = Prices {
            // SuperCheap
            list: pr_list_descending(0.0),
            dt: now,
            interval: INTERVAL,
        };
        let ps = PersistentState {
            min_charge_tomorrow: 72,
        };
        let battery_level = 10u8;
        let cr = prices_to_charge_request(
            TeslamateId::testing_value(),
            &summary,
            battery_level,
            false,
            &ps,
            now,
            &Utc,
        );
        assert_eq!(cr.time, now);
        assert_eq!(cr.battery_level, battery_level);
        assert_eq!(cr.min_charge_tomorrow, 72);
        assert_eq!(cr.cheapest_price, expected_price);
        assert_eq!(cr.current_price, expected_current_price);
        if forced {
            assert_eq!(cr.force, Some(ChargeRequest::ChargeTo(72)));
        } else {
            assert_eq!(cr.force, None);
        }
        assert_eq!(cr.result, ChargeRequest::ChargeTo(90));
        assert_eq!(cr.normal, ChargeRequest::ChargeTo(90));
    }

    #[test]
    fn test_estimate_charge_time() {
        assert_eq!(None, estimate_charge_time(70, 70));
        assert_eq!(None, estimate_charge_time(100, 70));
        assert_eq!(Some(TimeDelta::minutes(280)), estimate_charge_time(51, 90));
    }
}
