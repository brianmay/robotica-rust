use crate::{amber::combined, tesla::TeslamateId, InitState};

use super::{rules, user_plan::UserPlan, Prices};
use chrono::{DateTime, Local, NaiveTime, TimeDelta, TimeZone, Utc};
use opentelemetry::metrics::Meter;
use robotica_backend::{
    pipes::{
        stateful::{self, create_pipe, Receiver},
        stateless, Subscriber, Subscription,
    },
    spawn,
};
use robotica_common::{
    datetime::time_delta,
    mqtt::{Json, MqttMessage, Parsed, QoS, Retain},
    unsafe_naive_time_hms,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::select;
use tracing::{debug, error, info};

#[derive(Copy, Clone, Eq, PartialEq, Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "value")]
#[serde(rename_all = "snake_case")]
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

impl combined::RequestTrait for ChargeRequest {
    type GaugeType = u64;

    fn init_gauge(meter: &Meter) -> opentelemetry::metrics::Gauge<Self::GaugeType> {
        meter.u64_gauge("charge_request").init()
    }

    fn get_meter_value(&self) -> Self::GaugeType {
        match self {
            Self::ChargeTo(limit) => u64::from(*limit),
            Self::Manual => 0,
        }
    }

    fn get_nil_meter_value() -> Self::GaugeType {
        0
    }
}

impl Default for ChargeRequest {
    fn default() -> Self {
        Self::ChargeTo(0)
    }
}

impl combined::Max for ChargeRequest {
    fn max(self, other: Self) -> Self {
        match (self, other) {
            (Self::ChargeTo(a), Self::ChargeTo(b)) => Self::ChargeTo(a.max(b)),
            (Self::ChargeTo(a), Self::Manual) => Self::ChargeTo(a),
            (Self::Manual, Self::ChargeTo(b)) => Self::ChargeTo(b),
            (Self::Manual, Self::Manual) => Self::Manual,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct ChargePlanUserData {
    min_charge_tomorrow: u8,
}

impl ChargePlanUserData {
    const fn new(ps: &PersistentState) -> Self {
        Self {
            min_charge_tomorrow: ps.min_charge_tomorrow,
        }
    }
}

type ChargePlan = UserPlan<ChargePlanUserData>;

impl ChargePlan {
    const fn none_from_ps(ps: &PersistentState) -> Self {
        Self::new_none(ChargePlanUserData::new(ps))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistentState {
    min_charge_tomorrow: u8,
    charge_plan: ChargePlan,
    rules: rules::RuleSet<ChargeRequest>,
}

impl Default for PersistentState {
    fn default() -> Self {
        let min_charge_tomorrow = 70;
        Self {
            min_charge_tomorrow,
            charge_plan: UserPlan::new_none(ChargePlanUserData {
                min_charge_tomorrow,
            }),
            rules: rules::RuleSet::new(vec![]),
        }
    }
}

// Refactoring this is on TODO list.
#[allow(clippy::too_many_arguments)]
pub fn run(
    state: &InitState,
    teslamate_id: TeslamateId,
    rx: Receiver<Arc<Prices>>,
    battery_level: stateful::Receiver<Parsed<u8>>,
    min_charge_tomorrow: stateless::Receiver<Parsed<u8>>,
    is_charging: stateful::Receiver<bool>,
    rules: stateless::Receiver<Json<rules::RuleSet<ChargeRequest>>>,
) -> Receiver<ChargeRequest> {
    let (tx_out, rx_out) = create_pipe("amber/car");
    let id = format!(
        "tesla/{teslamate_id}",
        teslamate_id = teslamate_id.to_string()
    );
    let mqtt = state.mqtt.clone();

    let psr = state
        .persistent_state_database
        .for_name::<PersistentState>(&format!("tesla_amber_{id}"));
    let mut ps = psr.load().unwrap_or_default();

    let meters: combined::Meters<ChargeRequest> = combined::Meters::new(&id);

    spawn(async move {
        let mut s = rx.subscribe().await;
        let mut s_min_charge_tomorrow = min_charge_tomorrow.subscribe().await;
        let mut s_battery_level = battery_level.subscribe().await;
        let mut s_is_charging = is_charging.subscribe().await;
        let mut s_rules = rules.subscribe().await;

        let Ok(mut v_prices) = s.recv().await else {
            error!(id, "Failed to get initial prices");
            return;
        };
        let Ok(mut v_battery_level) = s_battery_level.recv().await.as_deref().copied() else {
            error!(id, "Failed to get initial battery level");
            return;
        };
        let Ok(mut v_is_charging) = s_is_charging.recv().await else {
            error!(id, "Failed to get initial charging state");
            return;
        };

        loop {
            info!(id, ?ps, "Persistent State");
            let (request, new_ps) = prices_to_charge_request(
                &id,
                &v_prices,
                v_battery_level,
                v_is_charging,
                ps,
                Some(&meters),
                Utc::now(),
                &Local,
            );
            ps = new_ps;

            save_state(teslamate_id, &psr, &ps);

            info!(id, request=?request, "Charging request");
            publish_state(teslamate_id, &request, &mqtt);
            tx_out.try_send(request.combined.get_result());

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
                    debug!(id, min_charge_tomorrow = *min_charge_tomorrow, "Setting min charge tomorrow");
                    ps.min_charge_tomorrow = *min_charge_tomorrow;
                    save_state(teslamate_id, &psr, &ps);
                },
                Ok(rules) = s_rules.recv() => {
                    debug!(id, ?rules, "Setting rules");
                    ps.rules = rules.into_inner();
                    save_state(teslamate_id, &psr, &ps);
                },
                Some(()) = ps.charge_plan.sleep_until_plan_start() => {
                    info!(id, "Plan start time elapsed");
                },
                Some(()) = ps.charge_plan.sleep_until_plan_end() => {
                    info!(id, "Plan end time elapsed");
                    if v_is_charging {
                        info!(id, "Plan ended, but was still charging, estimated time was too short");
                    }
                    ps.charge_plan = UserPlan::none_from_ps(&ps);
                },
                else => break,
            }
        }
    });

    rx_out
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

#[allow(clippy::too_many_arguments)]
fn prices_to_charge_request<T: TimeZone>(
    id: &str,
    prices: &Prices,
    battery_level: u8,
    is_charging: bool,
    mut ps: PersistentState,
    meters: Option<&combined::Meters<ChargeRequest>>,
    now: DateTime<Utc>,
    timezone: &T,
) -> (State, PersistentState) {
    // How long should car take to charge to min_charge_tomorrow?
    let estimated_charge_time_to_min =
        estimate_to_limit(id, battery_level, now, ps.min_charge_tomorrow, timezone);

    ps.charge_plan = if let Some(estimated_charge_time_to_min) = estimated_charge_time_to_min {
        let (_start_time, end_time) = super::private::get_day(now, END_TIME, timezone);
        let user_data = ChargePlanUserData::new(&ps);
        ps.charge_plan.update_plan(
            id,
            7.680,
            prices,
            now,
            end_time,
            estimated_charge_time_to_min,
            user_data,
        )
    } else {
        UserPlan::none_from_ps(&ps)
    };

    let request = combined::get_request(
        id,
        ChargeRequest::ChargeTo(ps.min_charge_tomorrow),
        &ps.charge_plan,
        &ps.rules,
        prices,
        is_charging,
        meters,
        now,
        timezone,
    );

    // Get some more stats
    let estimated_charge_time_to_limit = match request.get_result() {
        ChargeRequest::ChargeTo(limit) => {
            estimate_to_limit(id, battery_level, now, limit, timezone)
        }
        ChargeRequest::Manual => None,
    };
    let estimated_charge_time_to_90 = estimate_to_limit(id, battery_level, now, 90, timezone);

    let state = State {
        combined: request,
        battery_level,
        min_charge_tomorrow: ps.min_charge_tomorrow,
        estimated_charge_time_to_min,
        estimated_charge_time_to_limit,
        estimated_charge_time_to_90,
    };

    (state, ps)
}

fn estimate_to_limit<T: TimeZone>(
    id: &str,
    battery_level: u8,
    dt: DateTime<Utc>,
    limit: u8,
    tz: &T,
) -> Option<TimeDelta> {
    let estimated_charge_time = estimate_charge_time(battery_level, limit);
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
    battery_level: u8,
    min_charge_tomorrow: u8,

    #[serde(flatten)]
    combined: combined::State<ChargePlanUserData, ChargeRequest>,

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
    let topic = format!(
        "robotica/state/tesla/{id}/amber",
        id = teslamate_id.to_string()
    );
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
        let charge_time = diff * 280 * 60 / 39;
        // Allow for 1 minute for car waking up
        let charge_time = charge_time + 300;
        Some(TimeDelta::seconds(charge_time))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]

    use super::*;

    #[test]
    fn test_estimate_charge_time() {
        assert_eq!(None, estimate_charge_time(70, 70));
        assert_eq!(None, estimate_charge_time(100, 70));
        assert_eq!(
            Some(TimeDelta::seconds(17100)),
            estimate_charge_time(51, 90)
        );
    }
}
