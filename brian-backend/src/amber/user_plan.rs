use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::{cmp::min, fmt::Debug, time::Duration};
use tokio::time::{sleep_until, Instant};
use tracing::{error, info};

use super::{
    plan::{get_cheapest, Plan},
    Prices,
};

pub trait PlanTrait {
    fn get_client() -> &'static str;
}

#[derive(Serialize, Deserialize)]
pub struct UserPlan<T> {
    plan: Option<Plan>,
    user_data: T,
}

impl<T> UserPlan<T> {
    pub const fn new_none(user_data: T) -> Self {
        Self {
            plan: None,
            user_data,
        }
    }

    #[cfg(test)]
    pub const fn new_test(
        kw: f32,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        user_data: T,
    ) -> Self {
        Self {
            plan: Some(Plan::new_test(kw, start_time, end_time)),
            user_data,
        }
    }

    #[cfg(test)]
    pub const fn get_plan(&self) -> Option<&Plan> {
        self.plan.as_ref()
    }

    pub fn is_current(&self, now: DateTime<Utc>) -> bool {
        self.plan
            .as_ref()
            .map_or(false, |plan| plan.is_current(now))
    }
}

impl<T: Debug> Debug for UserPlan<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserPlan")
            .field("plan", &self.plan)
            .field("user_data", &self.user_data)
            .finish()
    }
}

impl<T: Clone> Clone for UserPlan<T> {
    fn clone(&self) -> Self {
        Self {
            plan: self.plan.clone(),
            user_data: self.user_data.clone(),
        }
    }
}

impl<T: PartialEq> PartialEq for UserPlan<T> {
    fn eq(&self, other: &Self) -> bool {
        self.plan == other.plan && self.user_data == other.user_data
    }
}

impl<T: Debug + PartialEq> UserPlan<T> {
    #[allow(clippy::cognitive_complexity)]
    pub fn update_plan(
        self,
        kw: f32,
        prices: &Prices,
        now: DateTime<Utc>,
        end_time: DateTime<Utc>,
        required_time_left: TimeDelta,
        user_data: T,
    ) -> Self
    where
        T: PlanTrait,
    {
        let old_user_plan = self;
        let client = T::get_client();

        // If required time left is negative or zero, then cancel the plan.
        if required_time_left <= TimeDelta::zero() {
            info!("Required time left is negative or zero");
            return Self {
                plan: None,
                user_data,
            };
        }

        let Some((new_plan, new_cost)) =
            get_cheapest(kw, now, end_time, required_time_left, prices)
        else {
            error!(client, plan =? old_user_plan, "Can't get new plan; using old plan");
            return old_user_plan;
        };

        let Some(old_plan) = old_user_plan.plan else {
            info!(client, plan =? new_plan, "No old plan available, using new Plan");
            return Self {
                plan: Some(new_plan),
                user_data,
            };
        };

        let Some(old_cost) = old_plan.get_forecast_cost(now, prices) else {
            info!(client, plan =? new_plan, "Old plan available but cannot get cost; using new plan");
            return Self {
                plan: Some(new_plan),
                user_data,
            };
        };

        // If there is more then 30 minutes left on plan and new plan is cheaper then 80% of old plan, then force new plan.
        // Or if the charge limit has changed, force new plan.
        let time_left = min(old_plan.get_end_time() - now, required_time_left);
        let threshold_reached = new_cost < old_cost * 0.8 && time_left >= TimeDelta::minutes(30);
        let has_changed = old_user_plan.user_data != user_data;
        let force = threshold_reached || has_changed;

        let old_plan_is_on = old_plan.is_current(now);
        let new_plan_is_on = new_plan.is_current(now);

        // If new plan continues old plan, use the old start time.
        let new_plan = if old_plan_is_on && new_plan_is_on {
            new_plan.with_start_time(old_plan.get_start_time())
        } else {
            new_plan
        };

        info!(
            client,
            ?old_plan,
            old_cost,
            old_plan_is_on,
            ?new_plan,
            new_cost,
            new_plan_is_on,
            threshold_reached,
            has_changed,
            force,
            "Choosing old plan or new plan"
        );

        #[allow(clippy::match_same_arms)]
        let use_new_plan = match (old_plan_is_on, new_plan_is_on, force) {
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
            info!(client, plan =? new_plan, "Using new plan");
            Self {
                plan: Some(new_plan),
                user_data,
            }
        } else {
            info!(client, plan =? old_plan, "Using old plan");
            Self {
                plan: Some(old_plan),
                user_data,
            }
        }
    }

    pub async fn sleep_until_plan_start(&self) -> Option<()>
    where
        T: Sync,
    {
        // If duration is negative, we can't sleep because this happened in the past.
        // This will always happen while plan is active.
        // In this case we return None.
        let start_time = self
            .plan
            .as_ref()
            .and_then(|plan| (plan.get_start_time() - Utc::now()).to_std().ok());

        if let Some(start_time) = start_time {
            sleep_until(Instant::now() + start_time).await;
            Some(())
        } else {
            None
        }
    }

    pub async fn sleep_until_plan_end(&self) -> Option<()>
    where
        T: Sync,
    {
        // If duration is negative, we can't sleep because this happened in the past.
        // In this case we return Some(()).
        // It is assumed the expired plan will be dropped.
        let end_time = self.plan.as_ref().map(|plan| {
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
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]

    use crate::amber::api::IntervalType;

    use super::*;
    use robotica_common::unsafe_duration;
    use std::time::Duration;

    const INTERVAL: Duration = unsafe_duration!(minutes: 30);

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    #[derive(Debug, PartialEq)]
    struct UserData {}

    impl PlanTrait for UserData {
        fn get_client() -> &'static str {
            "test"
        }
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

        let user_plan = UserPlan::new_none(UserData {});
        let user_plan = user_plan.update_plan(
            7.68,
            &prices,
            start_time,
            end_time,
            required_duration,
            UserData {},
        );

        let plan = user_plan.plan.unwrap();
        let cost = plan.get_forecast_cost(start_time, &prices).unwrap();
        assert_approx_eq!(f32, plan.get_kw(), 7.680);
        assert_eq!(plan.get_start_time(), expected_start_time);
        assert_eq!(plan.get_end_time(), expected_end_time);
        assert_approx_eq!(f32, cost, expected_cost);
    }
}
