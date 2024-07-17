use crate::{tesla::TeslamateId, InitState};

use super::{
    rules,
    user_plan::{PlanTrait, UserPlan},
    Prices,
};
use chrono::{DateTime, Local, NaiveTime, TimeDelta, TimeZone, Utc};
use opentelemetry::{global, KeyValue};
use robotica_backend::{
    pipes::{
        stateful::{self, create_pipe, Receiver},
        stateless, Subscriber, Subscription,
    },
    services::tesla::api::VehicleId,
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

#[derive(Debug)]
struct Meters {
    charging_requested: opentelemetry::metrics::Gauge<u64>,
    vehicle_id: VehicleId,
}

#[derive(Debug, Copy, Clone)]
enum ChargingReason {
    Plan,
    Rules,
    Combined,
}

impl Meters {
    fn new(vehicle_id: VehicleId) -> Self {
        let meter = global::meter("amber::car");

        Self {
            charging_requested: meter.u64_gauge("charging_requested").init(),
            vehicle_id,
        }
    }

    fn set_charging_requested(&self, request: ChargeRequest, reason: ChargingReason) {
        let reason = match reason {
            ChargingReason::Plan => "plan",
            ChargingReason::Rules => "cheap",
            ChargingReason::Combined => "combined",
        };
        let value = match request {
            ChargeRequest::ChargeTo(limit) => u64::from(limit),
            ChargeRequest::Manual => 0,
        };
        self.charging_requested.record(
            value,
            &[
                KeyValue::new("vehicle_id", self.vehicle_id.to_string()),
                KeyValue::new("reason", reason),
            ],
        );
    }

    fn set_nil_charging_requested(&self, reason: ChargingReason) {
        let reason = match reason {
            ChargingReason::Plan => "plan",
            ChargingReason::Rules => "cheap",
            ChargingReason::Combined => "combined",
        };
        let value = 0;
        self.charging_requested.record(
            value,
            &[
                KeyValue::new("vehicle_id", self.vehicle_id.to_string()),
                KeyValue::new("reason", reason),
            ],
        );
    }
}

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

    pub fn max(self, other: Self) -> Self {
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

impl PlanTrait for ChargePlanUserData {
    fn get_client() -> &'static str {
        "amber"
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
    tesla_id: VehicleId,
    rx: Receiver<Arc<Prices>>,
    battery_level: stateful::Receiver<Parsed<u8>>,
    min_charge_tomorrow: stateless::Receiver<Parsed<u8>>,
    is_charging: stateful::Receiver<bool>,
    rules: stateless::Receiver<Json<rules::RuleSet<ChargeRequest>>>,
) -> Receiver<ChargeRequest> {
    let (tx_out, rx_out) = create_pipe("amber/car");
    let id = teslamate_id.to_string();
    let mqtt = state.mqtt.clone();

    let psr = state
        .persistent_state_database
        .for_name::<PersistentState>(&format!("tesla_amber_{id}"));
    let mut ps = psr.load().unwrap_or_default();
    let meters = Meters::new(tesla_id);

    spawn(async move {
        let mut s = rx.subscribe().await;
        let mut s_min_charge_tomorrow = min_charge_tomorrow.subscribe().await;
        let mut s_battery_level = battery_level.subscribe().await;
        let mut s_is_charging = is_charging.subscribe().await;
        let mut s_rules = rules.subscribe().await;

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

        loop {
            info!("{id}: Persistent State: {:?}", ps);
            let (cr, new_ps) = prices_to_charge_request(
                teslamate_id,
                &v_prices,
                v_battery_level,
                v_is_charging,
                ps,
                Utc::now(),
                &Local,
            );
            ps = new_ps;
            if let Some(plan_request) = cr.plan_request {
                meters.set_charging_requested(plan_request, ChargingReason::Plan);
            } else {
                meters.set_nil_charging_requested(ChargingReason::Plan);
            }
            meters.set_charging_requested(cr.rules_request, ChargingReason::Rules);
            meters.set_charging_requested(cr.result, ChargingReason::Combined);

            save_state(teslamate_id, &psr, &ps);

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
                Ok(rules) = s_rules.recv() => {
                    debug!("{id}: Setting rules to {:?}", rules);
                    ps.rules = rules.into_inner();
                    save_state(teslamate_id, &psr, &ps);
                },
                Some(()) = ps.charge_plan.sleep_until_plan_start() => {
                    info!("{id}: Plan start time elapsed");
                },
                Some(()) = ps.charge_plan.sleep_until_plan_end() => {
                    info!("{id}: Plan end time elapsed");
                    if v_is_charging {
                        info!("{id}: Plan ended, but was still charging, estimated time was too short");
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

fn prices_to_charge_request<T: TimeZone>(
    teslamate_id: TeslamateId,
    prices: &Prices,
    battery_level: u8,
    is_charging: bool,
    mut ps: PersistentState,
    now: DateTime<Utc>,
    timezone: &T,
) -> (State, PersistentState) {
    let id = teslamate_id.to_string();

    // How long should car take to charge to min_charge_tomorrow?
    let estimated_charge_time_to_min = estimate_to_limit(
        teslamate_id,
        battery_level,
        now,
        ps.min_charge_tomorrow,
        timezone,
    );

    let plan_request = if let Some(estimated_charge_time_to_min) = estimated_charge_time_to_min {
        let (_start_time, end_time) = super::private::get_day(now, END_TIME, timezone);
        let user_data = ChargePlanUserData::new(&ps);
        let charge_plan = ps.charge_plan.update_plan(
            7.680,
            prices,
            now,
            end_time,
            estimated_charge_time_to_min,
            user_data,
        );
        let is_current = charge_plan.is_current(now);
        ps.charge_plan = charge_plan;
        if is_current {
            Some(ChargeRequest::ChargeTo(ps.min_charge_tomorrow))
        } else {
            None
        }
    } else {
        info!("{id}: No need to charge to {:?}", ps.min_charge_tomorrow);
        ps.charge_plan = UserPlan::none_from_ps(&ps);
        None
    };

    let rules_request = ps
        .rules
        .apply(prices, now, is_charging, timezone)
        .copied()
        .unwrap_or(ChargeRequest::ChargeTo(0));
    info!("{id}: Rules request: {rules_request:?}",);

    // get the largest value out of force and normal
    let combined_request = plan_request.map_or(rules_request, |force| {
        let result = rules_request.max(force);
        info!("{id}: Plan charge to {force:?}, now {result:?}");
        result
    });

    // Get some more stats
    let estimated_charge_time_to_limit = match combined_request {
        ChargeRequest::ChargeTo(limit) => {
            estimate_to_limit(teslamate_id, battery_level, now, limit, timezone)
        }
        ChargeRequest::Manual => None,
    };
    let estimated_charge_time_to_90 =
        estimate_to_limit(teslamate_id, battery_level, now, 90, timezone);

    let state = State {
        time: now,
        battery_level,
        min_charge_tomorrow: ps.min_charge_tomorrow,
        plan_request,
        rules_request,
        result: combined_request,
        charge_plan: ps.charge_plan.clone(),
        estimated_charge_time_to_min,
        estimated_charge_time_to_limit,
        estimated_charge_time_to_90,
        rules: ps.rules.clone(),
    };

    (state, ps)
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
    plan_request: Option<ChargeRequest>,
    rules_request: ChargeRequest,
    result: ChargeRequest,
    charge_plan: ChargePlan,

    #[serde(with = "robotica_common::datetime::with_option_time_delta")]
    estimated_charge_time_to_min: Option<TimeDelta>,

    #[serde(with = "robotica_common::datetime::with_option_time_delta")]
    estimated_charge_time_to_limit: Option<TimeDelta>,

    #[serde(with = "robotica_common::datetime::with_option_time_delta")]
    estimated_charge_time_to_90: Option<TimeDelta>,

    rules: rules::RuleSet<ChargeRequest>,
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

    use crate::amber::api::{self, IntervalType};

    use super::*;
    use float_cmp::approx_eq;
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

    #[rstest::rstest]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        true,
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T06:30:00Z"),
        22.0
    )]
    fn test_prices_to_charge_request(
        #[case] now: DateTime<Utc>,
        #[case] forced: bool,
        #[case] expected_start_time: DateTime<Utc>,
        #[case] expected_end_time: DateTime<Utc>,
        #[case] expected_cost: f32,
    ) {
        use tap::Pipe;

        let summary = Prices {
            list: pr_list_descending(0.0),
            interval: INTERVAL,
        };
        let rules = vec![
            rules::Rule::new(
                "is_on==true and weighted_price < 11.0".parse().unwrap(),
                ChargeRequest::ChargeTo(90),
            ),
            rules::Rule::new(
                "is_on==true and weighted_price < 16.0".parse().unwrap(),
                ChargeRequest::ChargeTo(80),
            ),
            rules::Rule::new(
                "is_on==true and weighted_price < 31.0".parse().unwrap(),
                ChargeRequest::ChargeTo(70),
            ),
            rules::Rule::new("is_on==true".parse().unwrap(), ChargeRequest::ChargeTo(50)),
            rules::Rule::new(
                "is_on==false and weighted_price < 9.0".parse().unwrap(),
                ChargeRequest::ChargeTo(90),
            ),
            rules::Rule::new(
                "is_on==false and weighted_price < 14.0".parse().unwrap(),
                ChargeRequest::ChargeTo(80),
            ),
            rules::Rule::new(
                "is_on==false and weighted_price < 29.0".parse().unwrap(),
                ChargeRequest::ChargeTo(70),
            ),
            rules::Rule::new("is_on==false".parse().unwrap(), ChargeRequest::ChargeTo(50)),
        ]
        .pipe(rules::RuleSet::new);

        let ps = PersistentState {
            min_charge_tomorrow: 72,
            charge_plan: UserPlan::new_none(ChargePlanUserData::new(&PersistentState::default())),
            rules,
        };
        let battery_level = 10u8;
        let (state, new_ps) = prices_to_charge_request(
            TeslamateId::testing_value(),
            &summary,
            battery_level,
            false,
            ps,
            now,
            &Utc,
        );
        assert_eq!(state.time, now);
        assert_eq!(state.battery_level, battery_level);
        assert_eq!(state.min_charge_tomorrow, 72);
        if forced {
            assert_eq!(state.plan_request, Some(ChargeRequest::ChargeTo(72)));
        } else {
            assert_eq!(state.plan_request, None);
        }
        assert_eq!(state.result, ChargeRequest::ChargeTo(90));
        assert_eq!(state.rules_request, ChargeRequest::ChargeTo(90));
        assert_eq!(new_ps.min_charge_tomorrow, 72);

        let charge_plan = new_ps.charge_plan.get_plan().unwrap();
        let cost = charge_plan.get_forecast_cost(now, &summary).unwrap();
        approx_eq!(f32, charge_plan.get_kw(), 7.68);
        assert_eq!(charge_plan.get_start_time(), expected_start_time);
        assert_eq!(charge_plan.get_end_time(), expected_end_time);
        approx_eq!(f32, cost, expected_cost);
    }

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
