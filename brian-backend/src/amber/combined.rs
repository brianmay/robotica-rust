use chrono::{DateTime, TimeZone, Utc};
use opentelemetry::{global, metrics::Meter, KeyValue};
use robotica_common::robotica::entities::Id;
use serde::Serialize;
use std::fmt::Debug;
use tracing::info;

use super::{rules::RuleSet, user_plan::MaybeUserPlan, Prices};

pub trait Max {
    fn max(self, other: Self) -> Self;
}

#[derive(Debug, Copy, Clone)]
enum Reason {
    Plan,
    Rules,
    Combined,
}

pub trait RequestTrait {
    type GaugeType;

    fn init_gauge(meter: &Meter) -> opentelemetry::metrics::Gauge<Self::GaugeType>;
    fn get_meter_value(&self) -> Self::GaugeType;
    fn get_nil_meter_value() -> Self::GaugeType;
}

#[derive(Debug)]
pub struct Meters<R: RequestTrait> {
    request: opentelemetry::metrics::Gauge<R::GaugeType>,
    id: Id,
    phantom: std::marker::PhantomData<R>,
}

impl<R: RequestTrait> Meters<R> {
    pub fn new(id: &Id) -> Self {
        let meter = global::meter("amber_combined");
        Self {
            request: R::init_gauge(&meter),
            id: id.clone(),
            phantom: std::marker::PhantomData,
        }
    }

    fn set_requested(&self, request: Option<R>, reason: Reason) {
        let reason = match reason {
            Reason::Plan => "plan",
            Reason::Rules => "cheap",
            Reason::Combined => "combined",
        };
        let value =
            request.map_or_else(R::get_nil_meter_value, |request| request.get_meter_value());
        self.request.record(
            value,
            &[
                KeyValue::new("id", self.id.to_string()),
                KeyValue::new("reason", reason),
            ],
        );
    }
}

#[derive(Debug, Serialize, PartialEq, Clone)]
pub struct State<R> {
    time: DateTime<Utc>,
    plan_request: Option<R>,
    rules_request: Option<R>,
    result: R,
    plan: MaybeUserPlan<R>,

    // #[serde(with = "robotica_common::datetime::with_option_time_delta")]
    // estimated_time_to_plan: Option<TimeDelta>,
    rules: RuleSet<R>,
}

impl<R> State<R> {
    // pub const fn get_plan(&self) -> &MaybeUserPlan<R> {
    //     &self.plan
    // }

    // pub const fn get_rules(&self) -> &RuleSet<R> {
    //     &self.rules
    // }

    // pub const fn get_estimated_time_to_plan(&self) -> Option<TimeDelta> {
    //     self.estimated_time_to_plan
    // }
}

impl<R: Copy> State<R> {
    pub const fn get_result(&self) -> R {
        self.result
    }
}

#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn get_request<R, TZ>(
    id: &Id,
    plan: &MaybeUserPlan<R>,
    rules: &RuleSet<R>,
    prices: &Prices,
    is_on: bool,
    meters: Option<&Meters<R>>,
    now: DateTime<Utc>,
    timezone: &TZ,
) -> State<R>
where
    R: Copy + Debug + Max + Default + RequestTrait,
    TZ: TimeZone,
{
    let rules_request = rules.apply(prices, now, is_on, timezone).copied();

    let plan_request = plan.get().and_then(|plan| {
        if plan.is_current(now) {
            Some(*plan.get_request())
        } else {
            None
        }
    });

    // let estimated_time_to_plan = plan.get_plan().map(|p| p.get_time_left(now));

    // get the largest value out of force and normal
    let combined_request = match (rules_request, plan_request) {
        (Some(rules_request), Some(plan_request)) => rules_request.max(plan_request),
        (Some(rules_request), None) => rules_request,
        (None, Some(plan_request)) => plan_request,
        (None, None) => R::default(),
    };

    info!(
        %id,
        ?plan_request,
        ?rules_request,
        ?combined_request,
        "combined request"
    );

    if let Some(meters) = meters {
        meters.set_requested(rules_request, Reason::Rules);
        meters.set_requested(plan_request, Reason::Plan);
        meters.set_requested(Some(combined_request), Reason::Combined);
    }

    State {
        time: now,
        result: combined_request,
        plan: plan.clone(),
        // estimated_time_to_plan,
        plan_request,
        rules: rules.clone(),
        rules_request,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]

    use crate::amber::api::{self, IntervalType};

    use super::*;
    use chrono::TimeDelta;
    use robotica_macro::duration_constant;
    use std::time::Duration;

    const INTERVAL: Duration = duration_constant!(30 minutes);

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
            date: start_time.with_timezone(&Utc).date_naive(),
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

    #[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
    struct TestRequest(u32);

    impl Max for TestRequest {
        fn max(self, other: Self) -> Self {
            Self(self.0.max(other.0))
        }
    }

    impl RequestTrait for TestRequest {
        type GaugeType = f64;

        fn init_gauge(meter: &Meter) -> opentelemetry::metrics::Gauge<Self::GaugeType> {
            meter.f64_gauge("test").with_description("test").build()
        }

        fn get_meter_value(&self) -> Self::GaugeType {
            f64::from(self.0)
        }

        fn get_nil_meter_value() -> Self::GaugeType {
            0.0
        }
    }

    #[rstest::rstest]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        Some(TestRequest(72)),
        Some(TestRequest(50)),
        TestRequest(72)
    )]
    fn test_get_request(
        #[case] now: DateTime<Utc>,
        #[case] expected_plan: Option<TestRequest>,
        #[case] expected_rules: Option<TestRequest>,
        #[case] expected_result: TestRequest,
    ) {
        use crate::amber::rules;
        use chrono::FixedOffset;
        use tap::Pipe;

        let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();

        let prices = Prices {
            list: pr_list_descending(100.0),
            interval: INTERVAL,
        };
        let rules = vec![
            rules::Rule::new(
                "is_on==true and weighted_price < 11.0".parse().unwrap(),
                TestRequest(90),
            ),
            rules::Rule::new(
                "is_on==true and weighted_price < 16.0".parse().unwrap(),
                TestRequest(80),
            ),
            rules::Rule::new(
                "is_on==true and weighted_price < 31.0".parse().unwrap(),
                TestRequest(70),
            ),
            rules::Rule::new("is_on==true".parse().unwrap(), TestRequest(50)),
            rules::Rule::new(
                "is_on==false and weighted_price < 9.0".parse().unwrap(),
                TestRequest(90),
            ),
            rules::Rule::new(
                "is_on==false and weighted_price < 14.0".parse().unwrap(),
                TestRequest(80),
            ),
            rules::Rule::new(
                "is_on==false and weighted_price < 29.0".parse().unwrap(),
                TestRequest(70),
            ),
            rules::Rule::new("is_on==false".parse().unwrap(), TestRequest(50)),
        ]
        .pipe(rules::RuleSet::new);

        let plan = MaybeUserPlan::new_test(10.0, now, now + TimeDelta::hours(6), TestRequest(72));
        let id = Id::new("test");

        let state = get_request(&id, &plan, &rules, &prices, false, None, now, &timezone);

        assert_eq!(state.time, now);
        assert_eq!(state.plan_request, expected_plan);
        assert_eq!(state.rules_request, expected_rules);
        assert_eq!(state.get_result(), expected_result);
        assert_eq!(state.plan, plan);
    }
}
