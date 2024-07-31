use super::{
    combined::{self},
    rules,
    user_plan::MaybeUserPlan,
    Prices,
};
use chrono::{DateTime, Local, NaiveTime, TimeDelta, TimeZone, Utc};
use opentelemetry::metrics::Meter;
use robotica_common::{
    datetime::{time_delta, utc_now},
    mqtt::Json,
};
use robotica_macro::time_delta_constant;
use robotica_tokio::{
    pipes::{
        stateful::{create_pipe, Receiver, Sender},
        stateless, Subscriber, Subscription,
    },
    services::persistent_state::{self, PersistentStateRow},
    spawn,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::select;
use tracing::{error, info};

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    Heat,
    DoNotHeat,
}

impl Default for Request {
    fn default() -> Self {
        Self::DoNotHeat
    }
}

impl combined::Max for Request {
    fn max(self, other: Self) -> Self {
        match (self, other) {
            (Self::Heat, _) | (_, Self::Heat) => Self::Heat,
            _ => Self::DoNotHeat,
        }
    }
}

impl combined::RequestTrait for Request {
    type GaugeType = u64;

    fn init_gauge(meter: &Meter) -> opentelemetry::metrics::Gauge<Self::GaugeType> {
        meter.u64_gauge("charge_request").init()
    }

    fn get_meter_value(&self) -> Self::GaugeType {
        match self {
            Self::Heat => 2,
            Self::DoNotHeat => 1,
        }
    }

    fn get_nil_meter_value() -> Self::GaugeType {
        0
    }
}

type HeatPlan = MaybeUserPlan<Request>;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct DayState {
    start: DateTime<Utc>,
    end: DateTime<Utc>,

    #[serde(with = "robotica_common::datetime::with_time_delta")]
    cheap_power_for_day: TimeDelta,
    last_cheap_update: DateTime<Utc>,
    is_on: bool,
    plan: HeatPlan,
    rules: rules::RuleSet<Request>,
}

const CHEAP_TIME: TimeDelta = time_delta_constant!(3 hours);

impl DayState {
    fn new<T: TimeZone>(now: DateTime<Utc>, timezone: &T) -> Self {
        let (start_day, end_day) = get_cheap_day(now, timezone);
        Self {
            start: start_day,
            end: end_day,
            cheap_power_for_day: TimeDelta::zero(),
            last_cheap_update: now,
            is_on: false,
            plan: HeatPlan::new_none(),
            rules: rules::RuleSet::new(vec![]),
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

    fn calculate_required_time_left<T: TimeZone>(
        &mut self,
        id: &str,
        now: DateTime<Utc>,
        cheap_time: TimeDelta,
        timezone: &T,
    ) -> TimeDelta {
        // If the date has changed, reset the cheap power for the day.
        if now < self.start || now >= self.end {
            let (start_day, end_day) = get_cheap_day(now, timezone);
            self.start = start_day;
            self.end = end_day;
            self.cheap_power_for_day = TimeDelta::zero();
            self.last_cheap_update = start_day;
            self.plan = HeatPlan::new_none();
        };

        // Add recent time to total cheap_power_for_day
        if self.is_on {
            let duration = now - self.last_cheap_update;
            info!(
                id,
                "Adding {duration:?} to cheap power for day {now:?} - {last_cheap_update:?}",
                last_cheap_update = self.last_cheap_update,
            );
            self.cheap_power_for_day += duration;
        }

        let duration = cheap_time
            .checked_sub(&self.cheap_power_for_day)
            .unwrap_or_else(TimeDelta::zero);

        info!(
            id,
            "Cheap power for day: {}, time left: {}",
            time_delta::to_string(self.cheap_power_for_day),
            time_delta::to_string(duration),
        );

        self.last_cheap_update = now;
        duration
    }
}

fn get_cheap_day<T: TimeZone>(now: DateTime<Utc>, local: &T) -> (DateTime<Utc>, DateTime<Utc>) {
    let end_time: NaiveTime = NaiveTime::from_hms_opt(15, 0, 0).unwrap_or_default();
    let (start_day, end_day) = super::private::get_day(now, end_time, local);
    (start_day, end_day)
}

#[derive(Clone, PartialEq, Serialize, Debug)]
pub struct State {
    #[serde(flatten)]
    pub combined: combined::State<Request>,
}

impl State {
    pub const fn get_result(&self) -> Request {
        self.combined.get_result()
    }
}

#[allow(clippy::too_many_arguments)]
fn process<T: TimeZone>(
    id: &str,
    mut day: DayState,
    prices: &Prices,
    tx_out: &Sender<State>,
    psr: &PersistentStateRow<DayState>,
    meters: Option<&combined::Meters<Request>>,
    now: DateTime<Utc>,
    timezone: &T,
) -> DayState {
    let maybe_new_plan = get_new_plan(&mut day, id, now, timezone, prices);
    let plan = day.plan.update_plan(id, prices, now, maybe_new_plan);

    let state = combined::get_request(
        id, &plan, &day.rules, prices, day.is_on, meters, now, timezone,
    );
    let request = state.get_result();

    let state = State { combined: state };

    info!(id, ?request, "Sending request");
    tx_out.try_send(state);
    day.plan = plan;
    day.save(psr);
    day
}

fn get_new_plan(
    day: &mut DayState,
    id: &str,
    now: DateTime<Utc>,
    timezone: &impl TimeZone,
    prices: &Prices,
) -> MaybeUserPlan<Request> {
    let required_time_left = day.calculate_required_time_left(id, now, CHEAP_TIME, timezone);
    MaybeUserPlan::get_cheapest(3.6, now, day.end, required_time_left, prices, Request::Heat)
}

pub fn run(
    persistent_state_database: &persistent_state::PersistentStateDatabase,
    rx: Receiver<Arc<Prices>>,
    is_on: Receiver<bool>,
    rules: stateless::Receiver<Json<rules::RuleSet<Request>>>,
) -> Receiver<State> {
    let (tx_out, rx_out) = create_pipe("amber/hot_water");
    let timezone = &Local;
    let id = "hot_water";

    let psr = persistent_state_database.for_name::<DayState>("hot_water_amber");

    let mut day = DayState::load(&psr, utc_now(), timezone);

    spawn(async move {
        let mut s = rx.subscribe().await;
        let mut s_is_on = is_on.subscribe().await;
        let mut s_rules = rules.subscribe().await;

        let Ok(mut prices) = s.recv().await else {
            error!(id, "Failed to get initial prices");
            return;
        };

        let meters = combined::Meters::new("hot_water");

        info!(id, "Received initial prices");
        day = process(
            id,
            day,
            &prices,
            &tx_out,
            &psr,
            Some(&meters),
            utc_now(),
            timezone,
        );

        loop {
            select! {
                Ok(is_on) = s_is_on.recv() => {
                    let _required_time = day.calculate_required_time_left(id, utc_now(), CHEAP_TIME, timezone);
                    day.is_on = is_on;
                    day.save(&psr);
                },
                Ok(new_prices) = s.recv() => {
                    info!(id, "Received new prices");
                    prices = new_prices;
                    day = process(id, day, &prices, &tx_out, &psr, Some(&meters), utc_now(), timezone);
                }
                Ok(Json(new_rules)) = s_rules.recv() => {
                    info!(id, "Received new rules");
                    day.rules = new_rules;
                    day = process(id, day, &prices, &tx_out, &psr, Some(&meters), utc_now(), timezone);
                }
                Some(()) = day.plan.sleep_until_plan_start() => {
                    info!(id, "Plan start time elapsed");
                    day = process(id, day, &prices, &tx_out, &psr, Some(&meters), utc_now(), timezone);
                }
                Some(()) = day.plan.sleep_until_plan_end() => {
                    info!(id, "Plan end time elapsed");
                    day.plan = HeatPlan::new_none();
                    day = process(id, day, &prices, &tx_out, &psr, Some(&meters), utc_now(), timezone);
                }
                else => break,
            }
        }
    });
    rx_out
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
    use robotica_macro::duration_constant;
    use std::time::Duration;

    use super::*;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    const INTERVAL: Duration = duration_constant!(30 minutes);

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
                plan: HeatPlan::new_none(),
                rules: rules::RuleSet::new(vec![]),
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
    fn test_calculate_required_time_left(
        #[case] now: DateTime<Utc>,
        #[case] last_cheap_update: DateTime<Utc>,
        #[case] cheap_power_for_day: TimeDelta,
        #[case] is_on: bool,
        #[case] expected_time_used: TimeDelta,
        #[case] expected_time_left: TimeDelta,
    ) {
        let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();
        let id = "test";

        let mut ds = DayState {
            start: dt("2019-12-31T04:00:00Z"),
            end: dt("2020-01-01T04:00:00Z"),
            last_cheap_update,
            cheap_power_for_day,
            is_on,
            plan: HeatPlan::new_none(),
            rules: rules::RuleSet::new(vec![]),
        };

        let cheap_time = TimeDelta::minutes(180);
        let actual = ds.calculate_required_time_left(id, now, cheap_time, &timezone);
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

        let maybe_new_plan = MaybeUserPlan::get_cheapest(
            3.6,
            start_time,
            end_time,
            required_duration,
            &prices,
            Request::Heat,
        );
        let user_plan = MaybeUserPlan::new_none();
        let user_plan = user_plan.update_plan("test", &prices, start_time, maybe_new_plan);

        let plan = user_plan.get_plan().unwrap();
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
        dt("2020-01-01T01:00:00Z"),
        false,
        Request::Heat
    )]
    #[case(
        dt("2020-01-01T01:00:50Z"),
        dt("2020-01-01T02:00:00Z"),
        dt("2020-01-01T01:00:50Z"),
        false,
        Request::Heat
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
    fn test_get_request(
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

        let rules = rules::RuleSet::new(vec![
            rules::Rule::new(
                "is_on==false and weighted_price < 12.0".parse().unwrap(),
                Request::Heat,
            ),
            rules::Rule::new(
                "is_on==true and weighted_price < 14.0".parse().unwrap(),
                Request::Heat,
            ),
            rules::Rule::new("true == true".parse().unwrap(), Request::DoNotHeat),
        ]);

        let plan = HeatPlan::new_test(3.6, start_time, end_time, Request::Heat);

        // Act
        let request =
            combined::get_request("test", &plan, &rules, &prices, is_on, None, now, &timezone)
                .get_result();

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
