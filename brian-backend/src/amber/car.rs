use crate::{tesla::TeslamateId, InitState};

use super::{
    plan::{get_cheapest, Plan},
    price_category::{get_weighted_price_category, PriceCategory},
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
    mqtt::{MqttMessage, Parsed, QoS, Retain},
    unsafe_naive_time_hms,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::{
    select,
    time::{sleep_until, Instant},
};
use tracing::{debug, error, info};

#[derive(Debug)]
struct Meters {
    charging_requested: opentelemetry::metrics::Gauge<u64>,
    vehicle_id: VehicleId,
}

#[derive(Debug, Copy, Clone)]
enum ChargingReason {
    Plan,
    Cheap,
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
            ChargingReason::Cheap => "cheap",
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
            ChargingReason::Cheap => "cheap",
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct ChargePlanState {
    plan: Plan,
    charge_limit: u8,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistentState {
    min_charge_tomorrow: u8,
    charge_plan: Option<ChargePlanState>,
}

impl Default for PersistentState {
    fn default() -> Self {
        Self {
            min_charge_tomorrow: 70,
            charge_plan: None,
        }
    }
}

async fn sleep_until_plan_start(plan: &Option<ChargePlanState>) -> Option<()> {
    // If duration is negative, we can't sleep because this happened in the past.
    // This will always happen while plan is active.
    // In this case we return None.
    let start_time = plan
        .as_ref()
        .and_then(|plan| (plan.plan.get_start_time() - Utc::now()).to_std().ok());

    if let Some(start_time) = start_time {
        sleep_until(Instant::now() + start_time).await;
        Some(())
    } else {
        None
    }
}

async fn sleep_until_plan_end(plan: &Option<ChargePlanState>) -> Option<()> {
    // If duration is negative, we can't sleep because this happened in the past.
    // In this case we return Some(()).
    // It is assumed the expired plan will be dropped.
    let end_time = plan.as_ref().map(|plan| {
        // If plan end time is in the past this will return immediately.
        (plan.plan.get_end_time() - Utc::now())
            .to_std()
            .unwrap_or_else(|_| Duration::from_secs(0))
    });

    if let Some(end_time) = end_time {
        sleep_until(Instant::now() + end_time).await;
        Some(())
    } else {
        None
    }
}

pub fn run(
    state: &InitState,
    teslamate_id: TeslamateId,
    tesla_id: VehicleId,
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
    let meters = Meters::new(tesla_id);

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
            meters.set_charging_requested(cr.cheap_request, ChargingReason::Cheap);
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
                Some(()) = sleep_until_plan_start(&ps.charge_plan) => {
                    info!("{id}: Plan start time elapsed");
                },
                Some(()) = sleep_until_plan_end(&ps.charge_plan) => {
                    info!("{id}: Plan end time elapsed");
                    if v_is_charging {
                        info!("{id}: Plan ended, but was still charging, estimated time was too short");
                    }
                    ps.charge_plan = None;
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
    tz: &T,
) -> (State, PersistentState) {
    let id = teslamate_id.to_string();

    // How long should car take to charge to min_charge_tomorrow?
    let estimated_charge_time_to_min =
        estimate_to_limit(teslamate_id, battery_level, now, ps.min_charge_tomorrow, tz);

    let plan_request = if let Some(estimated_charge_time_to_min) = estimated_charge_time_to_min {
        let (_start_time, end_time) = super::private::get_day(now, END_TIME, tz);
        let charge_plan = update_charge_plan(
            ps.charge_plan,
            prices,
            now,
            end_time,
            estimated_charge_time_to_min,
            ps.min_charge_tomorrow,
        );
        let is_current = charge_plan
            .as_ref()
            .map_or(false, |plan| plan.plan.is_current(now));
        ps.charge_plan = charge_plan;
        if is_current {
            Some(ChargeRequest::ChargeTo(ps.min_charge_tomorrow))
        } else {
            None
        }
    } else {
        info!("{id}: No need to charge to {:?}", ps.min_charge_tomorrow);
        ps.charge_plan = None;
        None
    };

    // Get the normal charge request based on category
    let category = get_weighted_price_category(is_charging, prices, now);
    #[allow(clippy::match_same_arms)]
    let cheap_request = match category {
        Some(PriceCategory::SuperCheap) => ChargeRequest::ChargeTo(90),
        Some(PriceCategory::Cheap) => ChargeRequest::ChargeTo(80),
        Some(PriceCategory::Normal) => ChargeRequest::ChargeTo(50),
        Some(PriceCategory::Expensive) => ChargeRequest::ChargeTo(20),
        None => ChargeRequest::ChargeTo(50),
    };
    info!("{id}: Price Category: {category:?}, Cheap request: {cheap_request:?}",);

    // get the largest value out of force and normal
    let combined_request = plan_request.map_or(cheap_request, |force| {
        let result = cheap_request.max(force);
        info!("{id}: Plan charge to {force:?}, now {result:?}");
        result
    });

    // Get some more stats
    let estimated_charge_time_to_limit = match combined_request {
        ChargeRequest::ChargeTo(limit) => {
            estimate_to_limit(teslamate_id, battery_level, now, limit, tz)
        }
        ChargeRequest::Manual => None,
    };
    let estimated_charge_time_to_90 = estimate_to_limit(teslamate_id, battery_level, now, 90, tz);

    let state = State {
        time: now,
        battery_level,
        min_charge_tomorrow: ps.min_charge_tomorrow,
        plan_request,
        cheap_request,
        result: combined_request,
        charge_plan: ps.charge_plan.clone(),
        estimated_charge_time_to_min,
        estimated_charge_time_to_limit,
        estimated_charge_time_to_90,
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
    cheap_request: ChargeRequest,
    result: ChargeRequest,
    charge_plan: Option<ChargePlanState>,

    #[serde(with = "robotica_common::datetime::with_option_time_delta")]
    estimated_charge_time_to_min: Option<TimeDelta>,

    #[serde(with = "robotica_common::datetime::with_option_time_delta")]
    estimated_charge_time_to_limit: Option<TimeDelta>,

    #[serde(with = "robotica_common::datetime::with_option_time_delta")]
    estimated_charge_time_to_90: Option<TimeDelta>,
}

#[allow(clippy::cognitive_complexity)]
fn update_charge_plan(
    plan: Option<ChargePlanState>,
    prices: &Prices,
    now: DateTime<Utc>,
    end_time: DateTime<Utc>,
    required_time_left: TimeDelta,
    charge_limit: u8,
) -> Option<ChargePlanState> {
    // If required time left is negative or zero, then cancel the plan.
    if required_time_left <= TimeDelta::zero() {
        info!("Required time left is negative or zero");
        return None;
    }

    let Some((new_plan, new_cost)) = get_cheapest(7.68, now, end_time, required_time_left, prices)
    else {
        error!("Can't get new plan");
        return plan;
    };

    let new_plan = ChargePlanState {
        plan: new_plan,
        charge_limit,
    };

    let plan_cost = if let Some(plan) = plan {
        info!("Old Plan: {plan:?}, checking cost");
        plan.plan.get_forecast_cost(now, prices).map_or_else(
            || {
                info!("Old plan available but cannot get cost");
                None
            },
            |cost| Some((plan, cost)),
        )
    } else {
        info!("No old plan available");
        None
    };

    if let Some((plan, cost)) = plan_cost {
        let threshold_reached = new_cost < cost * 0.8;
        let has_changed = plan.charge_limit != charge_limit;

        let plan_is_on = plan.plan.is_current(now);
        let new_plan_is_on = new_plan.plan.is_current(now);

        // If new plan continues old plan, use the old start time.
        let new_plan = if plan_is_on && new_plan_is_on {
            ChargePlanState {
                plan: new_plan.plan.with_start_time(plan.plan.get_start_time()),
                charge_limit,
            }
        } else {
            new_plan
        };

        let force = threshold_reached || has_changed;

        info!("Old Plan: {plan:?} {cost} {plan_is_on}");
        info!("New Plan: {new_plan:?} {new_cost} {new_plan_is_on}");
        info!("Threshold reached: {threshold_reached}");
        info!("Has changed: {has_changed}");

        #[allow(clippy::match_same_arms)]
        let use_new_plan = match (plan_is_on, new_plan_is_on, force) {
            // force criteria met, use new plan
            (_, _, true) => true,

            // Turning off but not meeting threshold, don't change
            (true, false, false) => false,

            // Already off, use new plan
            (false, _, false) => true,

            // Already on and staying on, use new plan
            (true, true, false) => true,
        };

        if use_new_plan {
            info!("Using new plan");
            Some(new_plan)
        } else {
            info!("Using old plan");
            Some(plan)
        }
    } else {
        info!("No old plan; Using new Plan: {:?}", new_plan);
        Some(new_plan)
    }
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
        dt("2020-01-01T05:30:00Z"),
        TimeDelta::minutes(120),
        dt("2020-01-01T02:00:00Z"),
        dt("2020-01-01T04:00:00Z"),
        307.19995
    )]
    fn test_update_charge_plan(
        #[case] start_time: DateTime<Utc>,
        #[case] end_time: DateTime<Utc>,
        #[case] required_duration: TimeDelta,
        #[case] expected_start_time: DateTime<Utc>,
        #[case] expected_end_time: DateTime<Utc>,
        #[case] expected_cost: f32,
    ) {
        use chrono::FixedOffset;
        use float_cmp::assert_approx_eq;

        use crate::amber::api::{ChannelType, PeriodType, PriceResponse, TariffInformation};

        let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();

        let pr = |start_time: DateTime<Utc>, price, interval_type| {
            let date = start_time.with_timezone(&timezone).date_naive();
            let end_time = start_time + INTERVAL;
            PriceResponse {
                date,
                start_time,
                end_time,
                per_kwh: price,
                spot_per_kwh: price,
                interval_type,
                renewables: 0.0,
                duration: 0,
                channel_type: ChannelType::General,
                estimate: Some(false),
                spike_status: "None".to_string(),
                tariff_information: TariffInformation {
                    period: PeriodType::Peak,
                    season: None,
                    block: None,
                    demand_window: None,
                },
            }
        };

        let prices = vec![
            pr(
                dt("2020-01-01T00:00:00Z"),
                30.0,
                IntervalType::ActualInterval,
            ),
            pr(
                dt("2020-01-01T00:30:00Z"),
                30.0,
                IntervalType::ActualInterval,
            ),
            pr(
                dt("2020-01-01T01:00:00Z"),
                30.0,
                IntervalType::CurrentInterval,
            ),
            pr(
                dt("2020-01-01T01:30:00Z"),
                30.0,
                IntervalType::ForecastInterval,
            ),
            pr(
                dt("2020-01-01T02:00:00Z"),
                20.0,
                IntervalType::ForecastInterval,
            ),
            pr(
                dt("2020-01-01T02:30:00Z"),
                20.0,
                IntervalType::ForecastInterval,
            ),
            pr(
                dt("2020-01-01T03:00:00Z"),
                20.0,
                IntervalType::ForecastInterval,
            ),
            pr(
                dt("2020-01-01T03:30:00Z"),
                20.0,
                IntervalType::ForecastInterval,
            ),
            pr(
                dt("2020-01-01T04:00:00Z"),
                30.0,
                IntervalType::ForecastInterval,
            ),
            pr(
                dt("2020-01-01T04:30:00Z"),
                30.0,
                IntervalType::ForecastInterval,
            ),
            pr(
                dt("2020-01-01T05:00:00Z"),
                30.0,
                IntervalType::ForecastInterval,
            ),
        ];

        let prices = Prices {
            list: prices,
            interval: INTERVAL,
        };

        let plan =
            update_charge_plan(None, &prices, start_time, end_time, required_duration, 10).unwrap();
        let cost = plan.plan.get_forecast_cost(start_time, &prices).unwrap();

        assert_approx_eq!(f32, plan.plan.get_kw(), 7.680);
        assert_eq!(plan.plan.get_start_time(), expected_start_time);
        assert_eq!(plan.plan.get_end_time(), expected_end_time);
        assert_approx_eq!(f32, cost, expected_cost);
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
        let summary = Prices {
            list: pr_list_descending(0.0),
            interval: INTERVAL,
        };
        let ps = PersistentState {
            min_charge_tomorrow: 72,
            charge_plan: None,
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
        assert_eq!(state.cheap_request, ChargeRequest::ChargeTo(90));
        assert_eq!(new_ps.min_charge_tomorrow, 72);

        let charge_plan = new_ps.charge_plan.unwrap();
        let cost = charge_plan.plan.get_forecast_cost(now, &summary).unwrap();
        approx_eq!(f32, charge_plan.plan.get_kw(), 7.68);
        assert_eq!(charge_plan.plan.get_start_time(), expected_start_time);
        assert_eq!(charge_plan.plan.get_end_time(), expected_end_time);
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
