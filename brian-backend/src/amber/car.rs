use crate::{delays::rate_limit, tesla::TeslamateId, InitState};

use super::{PriceCategory, Prices};
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

        let Ok(mut v_prices) = s.recv().await else {
            error!("{id}: Failed to get initial prices");
            return;
        };
        let Ok(mut v_battery_level) = s_battery_level.recv().await.as_deref().copied() else {
            error!("{id}: Failed to get initial battery level");
            return;
        };

        loop {
            let cr = prices_to_charge_request(
                teslamate_id,
                &v_prices,
                v_battery_level,
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
                Ok(min_charge_tomorrow) = s_min_charge_tomorrow.recv() => {
                    debug!("{id}: Setting min charge tomorrow to {}", *min_charge_tomorrow);
                    ps.min_charge_tomorrow = *min_charge_tomorrow;
                    psr.save(&ps).unwrap_or_else(|e| {
                        error!("{id}: Failed to save persistent state: {:?}", e);
                    });
                },
                else => break,
            }
        }
    });

    rate_limit("amber/car/ratelimit", Duration::from_secs(300), rx_out)
}

const END_TIME: NaiveTime = unsafe_naive_time_hms!(6, 30, 0);

fn prices_to_charge_request<T: TimeZone>(
    teslamate_id: TeslamateId,
    prices: &Prices,
    battery_level: u8,
    ps: &PersistentState,
    dt: DateTime<Utc>,
    tz: &T,
) -> State {
    let id = teslamate_id.to_string();

    let estimated_charge_time_to_min = estimate_charge_time(battery_level, ps.min_charge_tomorrow);
    let cheapest_price = estimated_charge_time_to_min.map_or_else(
        || {
            info!("{id}: Battery level is already at or above min charge",);
            None
        },
        |estimated_charge_time_to_min| {
            info!(
                "{id}: Estimated charge time to min is {}",
                time_delta::to_string(&estimated_charge_time_to_min)
            );

            let (_start_time, end_time) = super::private::get_day(&dt, END_TIME, tz);
            let cheapest_price = prices.get_cheapest_price_for_time_delta(
                estimated_charge_time_to_min,
                &dt,
                &end_time,
            );
            info!(
                "{dt} - {end_time}: Cheapest price is {cheapest_price:?}",
                dt = dt,
                end_time = end_time,
                cheapest_price = cheapest_price
            );
            cheapest_price
        },
    );

    let current_price = prices.current(&dt).map(|p| p.per_kwh);

    let force = match (cheapest_price, current_price) {
        (Some(cheapest_price), Some(current_price)) if current_price <= cheapest_price => {
            Some(ChargeRequest::ChargeTo(ps.min_charge_tomorrow))
        }
        _ => None,
    };

    #[allow(clippy::match_same_arms)]
    let normal = match prices.category {
        PriceCategory::SuperCheap => ChargeRequest::ChargeTo(90),
        PriceCategory::Cheap => ChargeRequest::ChargeTo(80),
        PriceCategory::Normal => ChargeRequest::ChargeTo(50),
        PriceCategory::Expensive => ChargeRequest::ChargeTo(20),
    };

    // get the largest value out of force and normal

    let result = match (force, normal) {
        (Some(force @ ChargeRequest::ChargeTo(f)), ChargeRequest::ChargeTo(n)) if f > n => force,
        (Some(ChargeRequest::Manual), _) | (_, ChargeRequest::Manual) => ChargeRequest::Manual,
        (_, normal) => normal,
    };

    let estimated_charge_time_to_limit = match result {
        ChargeRequest::ChargeTo(limit) => estimate_charge_time(battery_level, limit),
        ChargeRequest::Manual => None,
    };

    let estimated_charge_time_to_90 = estimate_charge_time(battery_level, 90);
    if let Some(estimated_charge_time_to_90) = estimated_charge_time_to_90 {
        info!(
            "{id}: Estimated charge time to 90% is {time}",
            id = id,
            time = time_delta::to_string(&estimated_charge_time_to_90)
        );
    }

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
    use test_log::test;

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

    fn pr_list() -> Vec<api::PriceResponse> {
        let time = dt("2020-01-01T00:00:00Z");

        (0i8..48i8)
            .map(|i| {
                let i64 = i64::from(i);
                let f32 = f32::from(i);
                pr(
                    time + TimeDelta::minutes(i64 * 30),
                    time + TimeDelta::minutes((i64 + 1) * 30),
                    IntervalType::ForecastInterval,
                    f32.mul_add(-0.5, 50.0),
                )
            })
            .collect::<Vec<api::PriceResponse>>()
    }

    #[test]
    fn test_prices_to_charge_request_normal_1() {
        let now = dt("2020-01-01T00:00:30Z");
        let summary = Prices {
            list: pr_list(),
            category: PriceCategory::SuperCheap,
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
            &ps,
            now,
            &Utc,
        );
        assert_eq!(
            cr,
            State {
                time: now,
                battery_level,
                min_charge_tomorrow: 0,
                cheapest_price: None,
                current_price: Some(50.0),
                force: None,
                normal: ChargeRequest::ChargeTo(90),
                result: ChargeRequest::ChargeTo(90),
                estimated_charge_time_to_min: None,
                estimated_charge_time_to_limit: Some(TimeDelta::seconds(8580)),
                estimated_charge_time_to_90: Some(TimeDelta::seconds(8580)),
            }
        );
    }

    #[test]
    fn test_prices_to_charge_request_normal_2() {
        let now = dt("2020-01-01T00:00:30Z");
        let summary = Prices {
            list: pr_list(),
            category: PriceCategory::Cheap,
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
            &ps,
            now,
            &Utc,
        );
        assert_eq!(
            cr,
            State {
                time: now,
                battery_level,
                min_charge_tomorrow: 0,
                cheapest_price: None,
                current_price: Some(50.0),
                force: None,
                normal: ChargeRequest::ChargeTo(80),
                result: ChargeRequest::ChargeTo(80),
                estimated_charge_time_to_min: None,
                estimated_charge_time_to_limit: Some(TimeDelta::seconds(4260)),
                estimated_charge_time_to_90: Some(TimeDelta::seconds(8580)),
            }
        );
    }

    #[test]
    fn test_prices_to_charge_request_normal_3() {
        let now = dt("2020-01-01T00:00:30Z");
        let summary = Prices {
            list: pr_list(),
            category: PriceCategory::Normal,
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
            &ps,
            now,
            &Utc,
        );
        assert_eq!(
            cr,
            State {
                time: now,
                battery_level,
                min_charge_tomorrow: 0,
                cheapest_price: None,
                current_price: Some(50.0),
                force: None,
                normal: ChargeRequest::ChargeTo(50),
                result: ChargeRequest::ChargeTo(50),
                estimated_charge_time_to_min: None,
                estimated_charge_time_to_limit: None,
                estimated_charge_time_to_90: Some(TimeDelta::seconds(8580)),
            }
        );
    }

    #[test]
    fn test_prices_to_charge_request_normal_4() {
        let now = dt("2020-01-01T00:00:30Z");
        let summary = Prices {
            list: pr_list(),
            category: PriceCategory::Expensive,
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
            &ps,
            now,
            &Utc,
        );
        assert_eq!(
            cr,
            State {
                time: now,
                battery_level,
                min_charge_tomorrow: 0,
                cheapest_price: None,
                current_price: Some(50.0),
                force: None,
                normal: ChargeRequest::ChargeTo(20),
                result: ChargeRequest::ChargeTo(20),
                estimated_charge_time_to_min: None,
                estimated_charge_time_to_limit: None,
                estimated_charge_time_to_90: Some(TimeDelta::seconds(8580)),
            }
        );
    }

    #[test]
    fn test_summary_to_charge_request_forced_1() {
        // threshold is 6:30am, this happens before that
        let now = dt("2020-01-01T03:30:30Z");
        let summary = Prices {
            list: pr_list(),
            category: PriceCategory::Expensive,
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
            &ps,
            now,
            &Utc,
        );
        assert_eq!(
            cr,
            State {
                time: now,
                battery_level,
                min_charge_tomorrow: 72,
                cheapest_price: Some(46.5),
                current_price: Some(46.5),
                force: Some(ChargeRequest::ChargeTo(72)),
                normal: ChargeRequest::ChargeTo(20),
                result: ChargeRequest::ChargeTo(72),
                estimated_charge_time_to_min: Some(TimeDelta::seconds(26700)),
                estimated_charge_time_to_limit: Some(TimeDelta::seconds(26700)),
                estimated_charge_time_to_90: Some(TimeDelta::seconds(34440)),
            }
        );
    }

    #[test]
    fn test_summary_to_charge_request_forced_2() {
        // threshold is 6:30am, this happens before that
        let now = dt("2020-01-01T06:00:30Z");
        let summary = Prices {
            list: pr_list(),
            category: PriceCategory::Expensive,
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
            &ps,
            now,
            &Utc,
        );
        assert_eq!(
            cr,
            State {
                time: now,
                battery_level,
                min_charge_tomorrow: 72,
                cheapest_price: Some(44.0),
                current_price: Some(44.0),
                force: Some(ChargeRequest::ChargeTo(72)),
                normal: ChargeRequest::ChargeTo(20),
                result: ChargeRequest::ChargeTo(72),
                estimated_charge_time_to_min: Some(TimeDelta::seconds(26700)),
                estimated_charge_time_to_limit: Some(TimeDelta::seconds(26700)),
                estimated_charge_time_to_90: Some(TimeDelta::seconds(34440)),
            }
        );
    }

    #[test]
    fn test_summary_to_charge_request_forced_3() {
        // threshold is 6:30am, this happens at after that
        let now = dt("2020-01-01T06:30:30Z");
        let summary = Prices {
            list: pr_list(),
            category: PriceCategory::Expensive,
            dt: now,
            interval: INTERVAL,
        };
        let ps = PersistentState {
            min_charge_tomorrow: 70,
        };
        let battery_level = 10u8;
        let cr = prices_to_charge_request(
            TeslamateId::testing_value(),
            &summary,
            battery_level,
            &ps,
            now,
            &Utc,
        );
        assert_eq!(
            cr,
            State {
                time: now,
                battery_level,
                min_charge_tomorrow: 70,
                cheapest_price: Some(33.5),
                current_price: Some(43.5),
                force: None,
                normal: ChargeRequest::ChargeTo(20),
                result: ChargeRequest::ChargeTo(20),
                estimated_charge_time_to_min: Some(TimeDelta::seconds(25800)),
                estimated_charge_time_to_limit: Some(TimeDelta::seconds(4260)),
                estimated_charge_time_to_90: Some(TimeDelta::seconds(34440)),
            }
        );
    }

    #[test]
    fn test_summary_to_charge_request_forced_4() {
        // threshold is 6:30am, this happens at after that
        let now = dt("2020-01-01T07:00:30Z");
        let summary = Prices {
            list: pr_list(),
            category: PriceCategory::Expensive,
            dt: now,
            interval: INTERVAL,
        };
        let ps = PersistentState {
            min_charge_tomorrow: 70,
        };
        let battery_level = 10u8;
        let cr = prices_to_charge_request(
            TeslamateId::testing_value(),
            &summary,
            battery_level,
            &ps,
            now,
            &Utc,
        );
        assert_eq!(
            cr,
            State {
                time: now,
                battery_level,
                min_charge_tomorrow: 70,
                cheapest_price: Some(33.5),
                current_price: Some(43.0),
                force: None,
                normal: ChargeRequest::ChargeTo(20),
                result: ChargeRequest::ChargeTo(20),
                estimated_charge_time_to_min: Some(TimeDelta::seconds(25800)),
                estimated_charge_time_to_limit: Some(TimeDelta::seconds(4260)),
                estimated_charge_time_to_90: Some(TimeDelta::seconds(34440)),
            }
        );
    }

    #[test]
    fn test_summary_to_charge_request_forced_1_but_cheap() {
        // threshold is 6:30am, this happens at before that
        let now = dt("2020-01-01T03:30:30Z");
        let summary = Prices {
            list: pr_list(),
            category: PriceCategory::SuperCheap,
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
            &ps,
            now,
            &Utc,
        );
        assert_eq!(
            cr,
            State {
                time: now,
                battery_level,
                min_charge_tomorrow: 72,
                cheapest_price: Some(46.5),
                current_price: Some(46.5),
                force: Some(ChargeRequest::ChargeTo(72)),
                normal: ChargeRequest::ChargeTo(90),
                result: ChargeRequest::ChargeTo(90),
                estimated_charge_time_to_min: Some(TimeDelta::seconds(26700)),
                estimated_charge_time_to_limit: Some(TimeDelta::seconds(34440)),
                estimated_charge_time_to_90: Some(TimeDelta::seconds(34440)),
            }
        );
    }

    #[test]
    fn test_summary_to_charge_request_forced_2_but_cheap() {
        // threshold is 6:30am, this happens at before that
        let now = dt("2020-01-01T06:00:30Z");
        let summary = Prices {
            list: pr_list(),
            category: PriceCategory::SuperCheap,
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
            &ps,
            now,
            &Utc,
        );
        assert_eq!(
            cr,
            State {
                time: now,
                battery_level,
                min_charge_tomorrow: 72,
                cheapest_price: Some(44.0),
                current_price: Some(44.0),
                force: Some(ChargeRequest::ChargeTo(72)),
                normal: ChargeRequest::ChargeTo(90),
                result: ChargeRequest::ChargeTo(90),
                estimated_charge_time_to_min: Some(TimeDelta::seconds(26700)),
                estimated_charge_time_to_limit: Some(TimeDelta::seconds(34440)),
                estimated_charge_time_to_90: Some(TimeDelta::seconds(34440))
            }
        );
    }

    #[test]
    fn test_summary_to_charge_request_forced_3_but_cheap() {
        // threshold is 6:30am, this happens at after that
        let now = dt("2020-01-01T06:30:30Z");
        let summary = Prices {
            list: pr_list(),
            category: PriceCategory::SuperCheap,
            dt: now,
            interval: INTERVAL,
        };
        let ps = PersistentState {
            min_charge_tomorrow: 70,
        };
        let battery_level = 10u8;
        let cr = prices_to_charge_request(
            TeslamateId::testing_value(),
            &summary,
            battery_level,
            &ps,
            now,
            &Utc,
        );
        assert_eq!(
            cr,
            State {
                time: now,
                battery_level,
                min_charge_tomorrow: 70,
                cheapest_price: Some(33.5),
                current_price: Some(43.5),
                force: None,
                normal: ChargeRequest::ChargeTo(90),
                result: ChargeRequest::ChargeTo(90),
                estimated_charge_time_to_min: Some(TimeDelta::seconds(25800)),
                estimated_charge_time_to_limit: Some(TimeDelta::seconds(34440)),
                estimated_charge_time_to_90: Some(TimeDelta::seconds(34440)),
            }
        );
    }

    #[test]
    fn test_summary_to_charge_request_forced_4_but_cheap() {
        // threshold is 6:30am, this happens at after that
        let now = dt("2020-01-01T07:00:30Z");
        let summary = Prices {
            list: pr_list(),
            category: PriceCategory::SuperCheap,
            dt: now,
            interval: INTERVAL,
        };
        let ps = PersistentState {
            min_charge_tomorrow: 70,
        };
        let battery_level = 10u8;
        let cr = prices_to_charge_request(
            TeslamateId::testing_value(),
            &summary,
            battery_level,
            &ps,
            now,
            &Utc,
        );
        assert_eq!(
            cr,
            State {
                time: now,
                battery_level,
                min_charge_tomorrow: 70,
                cheapest_price: Some(33.5),
                current_price: Some(43.0),
                force: None,
                normal: ChargeRequest::ChargeTo(90),
                result: ChargeRequest::ChargeTo(90),
                estimated_charge_time_to_min: Some(TimeDelta::seconds(25800)),
                estimated_charge_time_to_limit: Some(TimeDelta::seconds(34440)),
                estimated_charge_time_to_90: Some(TimeDelta::seconds(34440)),
            }
        );
    }

    #[test]
    fn test_estimate_charge_time() {
        assert_eq!(None, estimate_charge_time(70, 70));
        assert_eq!(None, estimate_charge_time(100, 70));
        assert_eq!(Some(TimeDelta::minutes(280)), estimate_charge_time(51, 90));
    }
}
