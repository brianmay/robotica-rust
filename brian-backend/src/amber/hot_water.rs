use crate::InitState;

use super::{
    plan::{self, get_cheapest},
    Prices,
};
use chrono::{DateTime, Local, NaiveTime, TimeDelta, TimeZone, Utc};
use plan::Plan;
use robotica_backend::{
    pipes::{
        stateful::{create_pipe, Receiver, Sender},
        Subscriber, Subscription,
    },
    services::persistent_state::PersistentStateRow,
    spawn,
};
use robotica_common::{
    datetime::{time_delta, utc_now},
    unsafe_time_delta,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::{cmp::min, sync::Arc};
use tokio::{
    select,
    time::{sleep_until, Instant},
};
use tracing::{error, info};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Request {
    Heat,
    DoNotHeat,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct DayState {
    start: DateTime<Utc>,
    end: DateTime<Utc>,

    #[serde(with = "robotica_common::datetime::with_time_delta")]
    cheap_power_for_day: TimeDelta,
    last_cheap_update: DateTime<Utc>,
    is_on: bool,
    plan: Option<Plan>,
}

const CHEAP_TIME: TimeDelta = unsafe_time_delta!(hours: 3);

impl DayState {
    fn new<T: TimeZone>(now: DateTime<Utc>, timezone: &T) -> Self {
        let (start_day, end_day) = get_cheap_day(now, timezone);
        Self {
            start: start_day,
            end: end_day,
            cheap_power_for_day: TimeDelta::zero(),
            last_cheap_update: now,
            is_on: false,
            plan: None,
        }
    }

    pub fn save(&self, psr: &PersistentStateRow<Self>) {
        psr.save(self).unwrap_or_else(|err| {
            error!("Failed to save day state: {}", err);
        });
    }

    pub fn load<T: TimeZone>(
        psr: &PersistentStateRow<Self>,
        now: DateTime<Utc>,
        timezone: &T,
    ) -> Self {
        psr.load().unwrap_or_else(|err| {
            error!("Failed to load day state, using defaults: {}", err);
            Self::new(now, timezone)
        })
    }

    fn cheap_update<T: TimeZone>(
        &mut self,
        now: DateTime<Utc>,
        cheap_time: TimeDelta,
        timezone: &T,
    ) -> TimeDelta {
        // If the date has changed, reset the cheap power for the day.
        if now < self.start || now >= self.end {
            *self = Self::new(now, timezone);
        };

        // Add recent time to total cheap_power_for_day
        if self.is_on {
            let duration = now - self.last_cheap_update;
            info!(
                "Adding {duration:?} to cheap power for day {now:?} - {last_cheap_update:?}",
                last_cheap_update = self.last_cheap_update,
            );
            self.cheap_power_for_day += duration;
        }

        let duration = cheap_time
            .checked_sub(&self.cheap_power_for_day)
            .unwrap_or_else(TimeDelta::zero);

        info!(
            "Cheap power for day: {}, time left: {}",
            time_delta::to_string(&self.cheap_power_for_day),
            time_delta::to_string(&duration),
        );

        self.last_cheap_update = now;
        duration
    }
}

#[allow(clippy::cognitive_complexity)]
fn update_plan(
    plan: Option<Plan>,
    prices: &Prices,
    now: DateTime<Utc>,
    end_time: DateTime<Utc>,
    required_time_left: TimeDelta,
) -> Option<Plan> {
    // If required time left is negative or zero, then cancel the plan.
    if required_time_left <= TimeDelta::zero() {
        info!("Required time left is negative or zero");
        return None;
    }

    let Some((new_plan, new_cost)) = get_cheapest(3.6, now, end_time, required_time_left, prices)
    else {
        error!("Can't get new plan");
        return plan;
    };

    let plan_cost = plan.map_or_else(
        || {
            info!("No old plan available");
            None
        },
        |plan| {
            info!("Old Plan: {plan:?}, checking cost");
            plan.get_forecast_cost(now, prices).map_or_else(
                || {
                    info!("Old plan available but cannot get cost");
                    None
                },
                |cost| Some((plan, cost)),
            )
        },
    );

    let new_plan_is_on = new_plan.is_current(now);

    if let Some((plan, cost)) = plan_cost {
        // If there is more then 30 minutes left on plan and new plan is cheaper then 80% of old plan, then force new plan.
        let time_left = min(plan.get_end_time() - now, required_time_left);
        let threshold_reached = new_cost < cost * 0.8 && time_left >= TimeDelta::minutes(30);
        let force = threshold_reached;

        let plan_is_on = plan.is_current(now);

        // If new plan continues old plan, use the old start time.
        let new_plan = if plan_is_on && new_plan_is_on {
            new_plan.with_start_time(plan.get_start_time())
        } else {
            new_plan
        };

        info!("Old Plan: {plan:?} {cost} {plan_is_on}");
        info!("New Plan: {new_plan:?} {new_cost} {new_plan_is_on}");
        info!("Threshold reached: {threshold_reached}");

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

fn prices_to_hot_water_request(
    is_on: bool,
    plan: &Option<Plan>,
    prices: &Prices,
    now: DateTime<Utc>,
) -> Request {
    let is_cheap = plan.as_ref().map_or(false, |plan| plan.is_current(now));

    let current_price = prices.get_weighted_price(now);
    let threshold = if is_on { 14.0 } else { 12.0 };

    let should_be_on = match (is_cheap, current_price) {
        (true, _) => true,
        (false, Some(price)) if price < threshold => true,
        _ => false,
    };

    if should_be_on {
        Request::Heat
    } else {
        Request::DoNotHeat
    }
}

fn get_cheap_day<T: TimeZone>(now: DateTime<Utc>, local: &T) -> (DateTime<Utc>, DateTime<Utc>) {
    let end_time: NaiveTime = NaiveTime::from_hms_opt(15, 0, 0).unwrap_or_default();
    let (start_day, end_day) = super::private::get_day(now, end_time, local);
    (start_day, end_day)
}

async fn sleep_until_plan_start(plan: &Option<Plan>) -> Option<()> {
    // If duration is negative, we can't sleep because this happened in the past.
    // This will always happen while plan is active.
    // In this case we return None.
    let start_time = plan.as_ref().and_then(|plan| {
        // If plan start time is in the past this will return None.
        (plan.get_start_time() - Utc::now()).to_std().ok()
    });

    if let Some(start_time) = start_time {
        sleep_until(Instant::now() + start_time).await;
        Some(())
    } else {
        None
    }
}

async fn sleep_until_plan_end(plan: &Option<Plan>) -> Option<()> {
    // If duration is negative, we can't sleep because this happened in the past.
    // In this case we return Some(()).
    // It is assumed the expired plan will be dropped.
    let end_time = plan.as_ref().map(|plan| {
        // If plan end time is in the past this will return immediately.
        (plan.get_end_time() - Utc::now())
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

fn process<T: TimeZone>(
    day: &mut DayState,
    prices: &Prices,
    tx_out: &Sender<Request>,
    psr: &PersistentStateRow<DayState>,
    timezone: &T,
) {
    let required_time_left = day.cheap_update(utc_now(), CHEAP_TIME, timezone);
    let plan = update_plan(
        day.plan.take(),
        prices,
        utc_now(),
        day.end,
        required_time_left,
    );
    let cr = prices_to_hot_water_request(day.is_on, &plan, prices, Utc::now());
    info!("Sending request: {:?}", cr);
    tx_out.try_send(cr);
    day.plan = plan;
    day.save(psr);
}

pub fn run(
    state: &InitState,
    rx: Receiver<Arc<Prices>>,
    is_on: Receiver<bool>,
) -> Receiver<Request> {
    let (tx_out, rx_out) = create_pipe("amber/hot_water");
    let timezone = &Local;

    let psr = state
        .persistent_state_database
        .for_name::<DayState>("hot_water_amber");

    let mut day = DayState::load(&psr, utc_now(), timezone);

    // Send initial state.
    {
        let initial = if day.is_on {
            Request::Heat
        } else {
            Request::DoNotHeat
        };
        info!("Sending initial state: {:?}", initial);
        tx_out.try_send(initial);
    }

    spawn(async move {
        let mut s = rx.subscribe().await;
        let mut s_is_on = is_on.subscribe().await;
        let Ok(mut prices) = s.recv().await else {
            error!("Failed to get initial prices");
            return;
        };

        info!("Received initial prices");
        process(&mut day, &prices, &tx_out, &psr, timezone);

        loop {
            select! {
                Ok(is_on) = s_is_on.recv() => {
                    let _required_time = day.cheap_update(utc_now(), CHEAP_TIME, timezone);
                    day.is_on = is_on;
                    day.save(&psr);
                },
                Ok(new_prices) = s.recv() => {
                    prices = new_prices;
                    info!("Received new prices");
                    process(&mut day, &prices, &tx_out, &psr, timezone);
                }
                Some(()) = sleep_until_plan_start(&day.plan) => {
                    info!("Plan start time elapsed");
                    process(&mut day, &prices, &tx_out, &psr, timezone);
                }
                Some(()) = sleep_until_plan_end(&day.plan) => {
                    info!("Plan end time elapsed");
                    day.plan = None;
                    process(&mut day, &prices, &tx_out, &psr, timezone);
                }
                else => break,
            }
        }
    });

    rx_out.rate_limit("amber/hot_water/ratelimit", Duration::from_secs(300))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]

    use crate::amber::api::{
        ChannelType, IntervalType, PeriodType, PriceResponse, TariffInformation,
    };
    use chrono::FixedOffset;
    use float_cmp::assert_approx_eq;
    use robotica_common::unsafe_duration;
    use std::time::Duration;

    use super::*;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    const INTERVAL: Duration = unsafe_duration!(minutes: 30);

    #[test]
    fn test_day_state_new() {
        let now = "2020-01-01T00:00:00Z".parse().unwrap();
        let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();
        let ds = DayState::new(now, &timezone);
        assert_eq!(
            ds,
            DayState {
                start: dt("2019-12-31T04:00:00Z"),
                end: dt("2020-01-01T04:00:00Z"),
                cheap_power_for_day: TimeDelta::minutes(0),
                last_cheap_update: now,
                is_on: false,
                plan: None
            }
        );
    }

    #[test_log::test(rstest::rstest)]
    #[case(
        dt("2020-01-01T00:30:00Z"),
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(0),
        false,
        TimeDelta::minutes(0),
        TimeDelta::minutes(180)
    )]
    #[case(
        dt("2020-01-01T00:30:00Z"),
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(0),
        true,
        TimeDelta::minutes(30),
        TimeDelta::minutes(150)
    )]
    #[case(
        dt("2020-01-01T00:30:00Z"),
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(12),
        false,
        TimeDelta::minutes(12),
        TimeDelta::minutes(180-12)
    )]
    #[case(
        dt("2020-01-01T00:30:00Z"),
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(12),
        true,
        TimeDelta::minutes(42),
        TimeDelta::minutes(180-42)
    )]
    fn test_cheap_update(
        #[case] now: DateTime<Utc>,
        #[case] last_cheap_update: DateTime<Utc>,
        #[case] cheap_power_for_day: TimeDelta,
        #[case] is_on: bool,
        #[case] expected_time_used: TimeDelta,
        #[case] expected_time_left: TimeDelta,
    ) {
        let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();
        let mut ds = DayState {
            start: dt("2019-12-31T04:00:00Z"),
            end: dt("2020-01-01T04:00:00Z"),
            last_cheap_update,
            cheap_power_for_day,
            is_on,
            plan: None,
        };

        let cheap_time = TimeDelta::minutes(180);
        let actual = ds.cheap_update(now, cheap_time, &timezone);
        assert_eq!(ds.last_cheap_update, now);
        assert_eq!(ds.cheap_power_for_day, expected_time_used);
        assert_eq!(actual, expected_time_left);
    }

    #[rstest::rstest]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T05:30:00Z"),
        TimeDelta::minutes(120),
        dt("2020-01-01T02:00:00Z"),
        dt("2020-01-01T04:00:00Z"),
        144.0
    )]
    fn test_update_plan(
        #[case] start_time: DateTime<Utc>,
        #[case] end_time: DateTime<Utc>,
        #[case] required_duration: TimeDelta,
        #[case] expected_start_time: DateTime<Utc>,
        #[case] expected_end_time: DateTime<Utc>,
        #[case] expected_cost: f32,
    ) {
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

        let plan = update_plan(None, &prices, start_time, end_time, required_duration).unwrap();
        let cost = plan.get_forecast_cost(start_time, &prices).unwrap();

        assert_approx_eq!(f32, plan.get_kw(), 3.6);
        assert_eq!(plan.get_start_time(), expected_start_time);
        assert_eq!(plan.get_end_time(), expected_end_time);
        assert_approx_eq!(f32, cost, expected_cost);
    }

    #[rstest::rstest]
    #[case(
        dt("2020-01-01T01:00:00Z"),
        dt("2020-01-01T02:00:00Z"),
        dt("2020-01-01T00:30:00Z"),
        false,
        Request::DoNotHeat
    )]
    #[case(
        dt("2020-01-01T01:00:00Z"),
        dt("2020-01-01T02:00:00Z"),
        dt("2020-01-01T01:30:00Z"),
        false,
        Request::Heat
    )]
    #[case(
        dt("2020-01-01T01:00:00Z"),
        dt("2020-01-01T02:00:00Z"),
        dt("2020-01-01T04:00:00Z"),
        false,
        Request::DoNotHeat
    )]
    #[case(
        dt("2020-01-01T01:00:00Z"),
        dt("2020-01-01T02:00:00Z"),
        dt("2020-01-01T03:00:00Z"),
        false,
        Request::Heat
    )]
    #[case(
        dt("2020-01-01T01:00:00Z"),
        dt("2020-01-01T02:00:00Z"),
        dt("2020-01-01T04:00:00Z"),
        false,
        Request::DoNotHeat
    )]
    #[case(
        dt("2020-01-01T01:00:00Z"),
        dt("2020-01-01T02:00:00Z"),
        dt("2020-01-01T03:00:00Z"),
        true,
        Request::Heat
    )]
    #[case(
        dt("2020-01-01T01:00:00Z"),
        dt("2020-01-01T02:00:00Z"),
        dt("2020-01-01T05:00:00Z"),
        false,
        Request::DoNotHeat
    )]
    #[case(
        dt("2020-01-01T01:00:00Z"),
        dt("2020-01-01T02:00:00Z"),
        dt("2020-01-01T04:00:00Z"),
        true,
        Request::Heat
    )]
    fn test_prices_to_hot_water_request(
        #[case] start_time: DateTime<Utc>,
        #[case] end_time: DateTime<Utc>,
        #[case] now: DateTime<Utc>,
        #[case] is_on: bool,
        #[case] expected: Request,
    ) {
        // Arrange
        use IntervalType::CurrentInterval;
        use IntervalType::ForecastInterval;
        let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();

        let tariff_information = TariffInformation {
            period: PeriodType::Peak,
            season: None,
            block: None,
            demand_window: None,
        };

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
                tariff_information: tariff_information.clone(),
            }
        };

        let prices = vec![
            pr(dt("2020-01-01T00:30:00Z"), 30.0, CurrentInterval),
            pr(dt("2020-01-01T01:00:00Z"), 30.0, ForecastInterval),
            pr(dt("2020-01-01T01:10:00Z"), 30.0, ForecastInterval),
            pr(dt("2020-01-01T01:30:00Z"), 40.0, ForecastInterval),
            pr(dt("2020-01-01T02:00:00Z"), 9.0, ForecastInterval),
            pr(dt("2020-01-01T02:30:00Z"), 10.0, ForecastInterval),
            pr(dt("2020-01-01T03:00:00Z"), 11.0, ForecastInterval),
            pr(dt("2020-01-01T03:30:00Z"), 12.0, ForecastInterval),
            pr(dt("2020-01-01T04:00:00Z"), 13.0, ForecastInterval),
            pr(dt("2020-01-01T04:30:00Z"), 14.0, ForecastInterval),
            pr(dt("2020-01-01T05:00:00Z"), 15.0, ForecastInterval),
            pr(dt("2020-01-01T05:30:00Z"), 16.0, ForecastInterval),
        ];

        let prices = Prices {
            list: prices,
            interval: INTERVAL,
        };

        let plan = Plan::new_test(3.6, start_time, end_time);

        // Act
        let request = prices_to_hot_water_request(is_on, &Some(plan), &prices, now);

        // Assert
        assert_eq!(request, expected);
    }

    #[test]
    fn test_get_cheap_day() {
        let timezone = FixedOffset::east_opt(60 * 60 * 11).unwrap();
        let now = dt("2020-01-02T00:00:00Z");
        let (start, stop) = get_cheap_day(now, &timezone);
        assert_eq!(start, dt("2020-01-01T04:00:00Z"));
        assert_eq!(stop, dt("2020-01-02T04:00:00Z"));
    }
}
