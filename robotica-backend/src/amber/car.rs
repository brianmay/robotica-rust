use crate::{amber::combined, car};

use super::{rules, user_plan::MaybeUserPlan, Prices};
use chrono::{DateTime, Local, NaiveTime, TimeDelta, TimeZone, Utc};
use opentelemetry::metrics::Meter;
use robotica_common::{
    datetime::time_delta,
    mqtt::{Json, Parsed},
    robotica::{amber::car::SetChargeEndTime, entities::Id},
};
use robotica_macro::naive_time_constant;
use robotica_tokio::{
    pipes::{
        stateful::{self, create_pipe, Receiver},
        stateless, Subscriber, Subscription,
    },
    spawn,
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::{
    select,
    time::{sleep_until, Instant},
};
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
        meter.u64_gauge("charge_request").build()
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

type ChargePlan = MaybeUserPlan<ChargeRequest>;

impl ChargePlan {
    const fn none() -> Self {
        Self::new_none()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistentState {
    min_charge_tomorrow: u8,
    charge_plan: ChargePlan,
    rules: rules::RuleSet<ChargeRequest>,
    charge_end_time: SetChargeEndTime,
}

impl PersistentState {
    pub async fn sleep_until_override_charge_expired(&self) {
        // If end time is in the past this will return immediately.
        let end_time = (self.charge_end_time.end_time - Utc::now())
            .to_std()
            .unwrap_or_else(|_| Duration::from_secs(0));

        sleep_until(Instant::now() + end_time).await;
    }
}

const END_TIME: NaiveTime = naive_time_constant!(06:30:0);

impl PersistentState {
    fn default_charge_end_time(
        now: DateTime<Utc>,
        min_charge_tomorrow: u8,
        timezone: &impl TimeZone,
    ) -> SetChargeEndTime {
        let end_time = {
            let (_start, end) = super::private::get_day(now, END_TIME, timezone);
            end
        };
        SetChargeEndTime {
            min_charge: min_charge_tomorrow,
            end_time,
        }
    }

    fn new(now: DateTime<Utc>, timezone: &impl TimeZone) -> Self {
        let min_charge_tomorrow = 70;
        Self {
            min_charge_tomorrow,
            charge_plan: MaybeUserPlan::new_none(),
            rules: rules::RuleSet::new(vec![]),
            charge_end_time: Self::default_charge_end_time(now, min_charge_tomorrow, timezone),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    car: &car::Config,
    persistent_state_database: &robotica_tokio::services::persistent_state::PersistentStateDatabase,
    rx: Receiver<Arc<Prices>>,
    battery_level: stateful::Receiver<Parsed<u8>>,
    min_charge_tomorrow: stateless::Receiver<Parsed<u8>>,
    set_charge_end_time: stateless::Receiver<Json<SetChargeEndTime>>,
    is_charging: stateful::Receiver<bool>,
    rules: stateless::Receiver<Json<rules::RuleSet<ChargeRequest>>>,
) -> Receiver<State> {
    let (tx_out, rx_out) = create_pipe("amber/car");
    let id = car.id.clone();

    let timezone = Local;

    let psr = persistent_state_database.for_name::<PersistentState>(&id, "amber_car");
    let mut ps = psr
        .load()
        .unwrap_or_else(|_| PersistentState::new(Utc::now(), &timezone));

    let meters: combined::Meters<ChargeRequest> = combined::Meters::new(&id);

    spawn(async move {
        let mut s = rx.subscribe().await;
        let mut s_min_charge_tomorrow = min_charge_tomorrow.subscribe().await;
        let mut s_set_charge_end_time = set_charge_end_time.subscribe().await;
        let mut s_battery_level = battery_level.subscribe().await;
        let mut s_is_charging = is_charging.subscribe().await;
        let mut s_rules = rules.subscribe().await;

        let Ok(mut v_prices) = s.recv().await else {
            error!(%id, "Failed to get initial prices");
            return;
        };
        let Ok(mut v_battery_level) = s_battery_level.recv().await.as_deref().copied() else {
            error!(%id, "Failed to get initial battery level");
            return;
        };
        let Ok(mut v_is_charging) = s_is_charging.recv().await else {
            error!(%id, "Failed to get initial charging state");
            return;
        };

        loop {
            info!(%id, ?ps, "Persistent State");
            let (request, new_ps) = prices_to_charge_request(
                &id,
                &v_prices,
                v_battery_level,
                v_is_charging,
                ps,
                Some(&meters),
                Utc::now(),
                &timezone,
            );
            ps = new_ps;

            save_state(&id, &psr, &ps);

            info!(%id, request=?request, "Charging request");
            tx_out.try_send(request);

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
                    debug!(%id, min_charge_tomorrow = *min_charge_tomorrow, "Setting min charge tomorrow");
                    ps.min_charge_tomorrow = *min_charge_tomorrow;
                    save_state(&id, &psr, &ps);
                },
                Ok(set_charge_end_time) = s_set_charge_end_time.recv() => {
                    let msg = set_charge_end_time.into_inner();
                    debug!(%id, override_min_charge = msg.min_charge, end_time = %msg.end_time, "Setting charge end time override");
                    ps.charge_end_time = msg;
                    save_state(&id, &psr, &ps);
                },
                Ok(rules) = s_rules.recv() => {
                    debug!(%id, ?rules, "Setting rules");
                    ps.rules = rules.into_inner();
                    save_state(&id, &psr, &ps);
                },
                Some(()) = ps.charge_plan.sleep_until_plan_start() => {
                    info!(%id, "Plan start time elapsed");
                },
                Some(()) = ps.charge_plan.sleep_until_plan_end() => {
                    info!(%id, "Plan end time elapsed");
                    if v_is_charging {
                        info!(%id, "Plan ended, but was still charging, estimated time was too short");
                    }
                    ps.charge_plan = MaybeUserPlan::none();
                },
                () = ps.sleep_until_override_charge_expired() => {
                    info!(%id, "Charge end time override expired");
                    ps.charge_end_time = PersistentState::default_charge_end_time(Utc::now(), ps.min_charge_tomorrow, &timezone);
                    save_state(&id, &psr, &ps);
                },
                else => break,
            }
        }
    });

    rx_out
}

fn save_state(
    id: &Id,
    psr: &robotica_tokio::services::persistent_state::PersistentStateRow<PersistentState>,
    ps: &PersistentState,
) {
    psr.save(ps).unwrap_or_else(|e| {
        error!(%id, "Failed to save persistent state: {:?}", e);
    });
}

#[allow(clippy::too_many_arguments)]
fn prices_to_charge_request<T: TimeZone>(
    id: &Id,
    prices: &Prices,
    battery_level: u8,
    is_charging: bool,
    mut ps: PersistentState,
    meters: Option<&combined::Meters<ChargeRequest>>,
    now: DateTime<Utc>,
    timezone: &T,
) -> (State, PersistentState) {
    let maybe_new_plan = get_new_plan(id, battery_level, now, &ps, timezone, prices);
    ps.charge_plan = ps.charge_plan.update_plan(id, prices, now, maybe_new_plan);

    let request = combined::get_request(
        id,
        &ps.charge_plan,
        &ps.rules,
        prices,
        is_charging,
        meters,
        now,
        timezone,
    );

    let state = State {
        combined: request,
        battery_level,
        min_charge_tomorrow: ps.min_charge_tomorrow,
        charge_end_time: ps.charge_end_time.clone(),
    };

    (state, ps)
}

fn get_new_plan_to_min_charge(
    id: &Id,
    battery_level: u8,
    now: DateTime<Utc>,
    timezone: &impl TimeZone,
    prices: &Prices,
    limit: u8,
    end_time: DateTime<Utc>,
) -> MaybeUserPlan<ChargeRequest> {
    let estimated_charge_time_to_min = estimate_to_limit(id, battery_level, now, limit, timezone);

    estimated_charge_time_to_min.map_or_else(MaybeUserPlan::none, |estimated_charge_time_to_min| {
        let request = ChargeRequest::ChargeTo(limit);
        MaybeUserPlan::get_cheapest(
            id,
            7.68,
            now,
            end_time,
            estimated_charge_time_to_min,
            prices,
            request,
        )
    })
}

#[allow(clippy::cognitive_complexity)]
fn get_new_plan(
    id: &Id,
    battery_level: u8,
    now: DateTime<Utc>,
    ps: &PersistentState,
    timezone: &impl TimeZone,
    prices: &Prices,
) -> MaybeUserPlan<ChargeRequest> {
    let charge_end_time = &ps.charge_end_time;

    let try_limits = {
        let mut limits = Vec::with_capacity(3);
        if charge_end_time.min_charge < 90 {
            limits.push((90, 7.68 * 10.0));
        }
        if charge_end_time.min_charge < 80 {
            limits.push((80, 7.68 * 15.0));
        }
        limits.push((charge_end_time.min_charge, 7.68 * 40.0));
        limits
    };

    for (limit, max_cost_per_hour) in try_limits {
        let new_plan = get_new_plan_to_min_charge(
            id,
            battery_level,
            now,
            timezone,
            prices,
            limit,
            charge_end_time.end_time,
        );
        if let Some(plan) = new_plan.get() {
            let user_plan_cost_per_hour = plan.get_average_cost_per_hour();
            let propose_plan = f64::from(user_plan_cost_per_hour) < max_cost_per_hour;

            if propose_plan {
                info!(
                    %id,
                    ?new_plan,
                    total_cost = new_plan.get_total_cost(),
                    average_cost_per_hour = user_plan_cost_per_hour,
                    max_cost_per_hour,
                    limit,
                    "Proposing new plan"
                );
                return new_plan;
            }

            info!(
                %id,
                ?new_plan,
                total_cost = new_plan.get_total_cost(),
                average_cost_per_hour = user_plan_cost_per_hour,
                max_cost_per_hour,
                limit,
                "Skipping plan as too expensive"
            );
        } else {
            info!(%id, limit, "No plan to charge to specified limit");
        }
    }

    info!(%id, "No plan found");
    MaybeUserPlan::none()
}

fn estimate_to_limit<T: TimeZone>(
    id: &Id,
    battery_level: u8,
    dt: DateTime<Utc>,
    limit: u8,
    tz: &T,
) -> Option<TimeDelta> {
    let estimated_charge_time = estimate_charge_time(battery_level, limit);
    if let Some(estimated_charge_time) = estimated_charge_time {
        let estimated_finish = dt + estimated_charge_time;
        debug!(
            %id,
            "Estimated charge time to {limit} is {time}, should finish at {finish:?}",
            time = time_delta::to_string(estimated_charge_time),
            finish = estimated_finish.with_timezone(tz).to_rfc3339()
        );
    } else {
        debug!(
            "{id}: Battery level is already at or above {limit}",
            limit = limit
        );
    }
    estimated_charge_time
}

#[derive(Debug, Serialize, PartialEq, Clone)]
pub struct State {
    pub battery_level: u8,
    pub min_charge_tomorrow: u8,

    #[serde(flatten)]
    pub combined: combined::State<ChargeRequest>,

    pub charge_end_time: SetChargeEndTime,
}

impl State {
    #[must_use]
    pub const fn get_result(&self) -> ChargeRequest {
        self.combined.get_result()
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
        // Allow for 5 minutes for car waking up
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
