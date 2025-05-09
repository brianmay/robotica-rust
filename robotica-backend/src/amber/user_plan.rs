use chrono::{DateTime, TimeDelta, Utc};
use robotica_common::robotica::entities::Id;
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, time::Duration};
use tokio::time::{sleep_until, Instant};
use tracing::info;

use super::{
    plan::{get_cheapest, Plan},
    Prices,
};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct UserPlan<R> {
    plan: Plan,
    request: R,
    cost: f32,
}

impl<R> UserPlan<R> {
    fn get_cheapest(
        id: &Id,
        kw: f32,
        start_search: DateTime<Utc>,
        end_search: DateTime<Utc>,
        required_duration: TimeDelta,
        prices: &Prices,
        request: R,
    ) -> Option<Self> {
        let plan = get_cheapest(id, kw, start_search, end_search, required_duration, prices);
        plan.map(|(plan, cost)| Self {
            plan,
            request,
            cost,
        })
    }

    pub fn with_start_time(self, start_time: DateTime<Utc>) -> Self {
        Self {
            plan: self.plan.with_start_time(start_time),
            request: self.request,
            cost: self.cost,
        }
    }

    pub const fn get_start_time(&self) -> DateTime<Utc> {
        self.plan.get_start_time()
    }

    pub const fn get_end_time(&self) -> DateTime<Utc> {
        self.plan.get_end_time()
    }

    // pub fn get_timedelta(&self) -> TimeDelta {
    //     self.plan.get_timedelta()
    // }

    pub fn get_time_left(&self, now: DateTime<Utc>) -> TimeDelta {
        self.plan.get_time_left(now)
    }

    pub fn is_current(&self, now: DateTime<Utc>) -> bool {
        self.plan.is_current(now)
    }

    #[cfg(test)]
    pub fn get_forecast_cost(&self, id: &Id, now: DateTime<Utc>, prices: &Prices) -> Option<f32> {
        self.plan.get_forecast_cost(id, now, prices)
    }

    pub fn get_forecast_average_cost(
        &self,
        id: &Id,
        now: DateTime<Utc>,
        prices: &Prices,
    ) -> Option<f32> {
        let duration = self.plan.get_timedelta();
        #[allow(clippy::cast_precision_loss)]
        let duration = duration.num_seconds() as f32 / 3600.0;
        self.plan
            .get_forecast_cost(id, now, prices)
            .map(|cost| cost / duration)
    }

    pub fn get_average_cost_per_hour(&self) -> f32 {
        let duration = self.plan.get_timedelta();
        #[allow(clippy::cast_precision_loss)]
        let duration = duration.num_seconds() as f32 / 3600.0;
        self.cost / duration
    }

    pub const fn get_total_cost(&self) -> f32 {
        self.cost
    }

    #[cfg(test)]
    pub const fn get_kw(&self) -> f32 {
        self.plan.get_kw()
    }

    pub const fn get_request(&self) -> &R {
        &self.request
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct MaybeUserPlan<R>(Option<UserPlan<R>>);

impl<R> MaybeUserPlan<R> {
    pub const fn new_none() -> Self {
        Self(None)
    }

    pub fn get_cheapest(
        id: &Id,
        kw: f32,
        start_search: DateTime<Utc>,
        end_search: DateTime<Utc>,
        required_duration: TimeDelta,
        prices: &Prices,
        request: R,
    ) -> Self {
        let maybe_plan = UserPlan::get_cheapest(
            id,
            kw,
            start_search,
            end_search,
            required_duration,
            prices,
            request,
        );
        Self(maybe_plan)
    }

    pub const fn get(&self) -> Option<&UserPlan<R>> {
        self.0.as_ref()
    }

    #[cfg(test)]
    pub const fn new_test(
        kw: f32,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        request: R,
    ) -> Self {
        let user_plan = UserPlan {
            plan: Plan::new_test(kw, start_time, end_time),
            request,
            cost: 0.0,
        };
        Self(Some(user_plan))
    }

    #[cfg(test)]
    pub const fn get_plan(&self) -> Option<&Plan> {
        if let Some(user_plan) = &self.0 {
            Some(&user_plan.plan)
        } else {
            None
        }
    }

    // pub fn is_current(&self, now: DateTime<Utc>) -> bool {
    //     self.get_plan().map_or(false, |plan| plan.is_current(now))
    // }

    pub fn get_average_cost_per_hour(&self) -> Option<f32> {
        self.0.as_ref().map(UserPlan::get_average_cost_per_hour)
    }

    pub fn get_total_cost(&self) -> Option<f32> {
        self.0.as_ref().map(UserPlan::get_total_cost)
    }
}

impl<T: Copy + Debug + PartialEq> MaybeUserPlan<T> {
    #[allow(clippy::cognitive_complexity)]
    // #[allow(clippy::too_many_arguments)]
    pub fn update_plan(
        self,
        id: &Id,
        prices: &Prices,
        now: DateTime<Utc>,
        maybe_new_user_plan: Self,
    ) -> Self {
        let old_user_plan = self;

        let Some(old_user_plan) = old_user_plan.0 else {
            info!(%id, plan = ?maybe_new_user_plan, "No old plan available, using new Plan");
            return maybe_new_user_plan;
        };

        let Some(old_average_cost) = old_user_plan.get_forecast_average_cost(id, now, prices)
        else {
            info!(%id, plan = ?maybe_new_user_plan, "Old plan available but cannot get cost; using new plan");
            return maybe_new_user_plan;
        };

        let Some(new_user_plan) = maybe_new_user_plan.0 else {
            // No new plan.
            // This has to be a special case; we can't use a nil plan because a nil plan has no average cost.
            let old_average_cost = old_user_plan.get_average_cost_per_hour();
            let new_current_cost = old_user_plan.get_forecast_average_cost(id, now, prices);
            let discard_plan = new_current_cost
                .is_none_or(|new_average_cost| old_average_cost > new_average_cost * 0.8);

            let result = if discard_plan {
                info!(%id, old_average_cost, new_current_cost, plan = ?old_user_plan, "No new plan available; price rise too much; discarding plan");
                Self(None)
            } else {
                info!(%id, old_average_cost, new_current_cost, plan = ?old_user_plan, "No new plan available; keeping old plan");
                Self(Some(old_user_plan))
            };
            return result;
        };

        let new_average_cost = new_user_plan.get_average_cost_per_hour();

        // If there is more then 30 minutes left on old plan
        // and new plan is cheaper then 80% of old plan, then force new plan.
        // Or if the charge limit has changed, force new plan.
        //
        // If either of the costs is NaN this means the period had no time,
        // so we should force the new plan.
        let time_left = old_user_plan.get_time_left(now);
        let threshold_reached = new_average_cost.is_nan()
            || old_average_cost.is_nan()
            || new_average_cost < old_average_cost * 0.8 && time_left >= TimeDelta::minutes(30);
        let has_changed = old_user_plan.request != new_user_plan.request;
        let force = threshold_reached || has_changed;

        let old_plan_is_on = old_user_plan.is_current(now);
        let new_plan_is_on = new_user_plan.is_current(now);

        // If new plan continues old plan, use the old start time.
        let new_user_plan = if old_plan_is_on && new_plan_is_on {
            new_user_plan.with_start_time(old_user_plan.get_start_time())
        } else {
            new_user_plan
        };

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
            info!(
                %id,
                ?old_user_plan,
                old_average_cost,
                old_plan_is_on,
                ?new_user_plan,
                new_average_cost = new_user_plan.get_average_cost_per_hour(),
                new_plan_is_on,
                threshold_reached,
                has_changed,
                force,
                plan=?new_user_plan,
                "Choosing new plan"
            );
            Self(Some(new_user_plan))
        } else {
            info!(
                %id,
                ?old_user_plan,
                old_average_cost,
                old_plan_is_on,
                ?new_user_plan,
                new_average_cost = new_user_plan.get_average_cost_per_hour(),
                new_plan_is_on,
                threshold_reached,
                has_changed,
                force,
                plan=?old_user_plan,
                "Choosing old plan"
            );
            Self(Some(old_user_plan))
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
            .0
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
        let end_time = self.0.as_ref().map(|plan| {
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

    use robotica_macro::duration_constant;

    use crate::amber::api::IntervalType;

    use super::*;
    use std::time::Duration;

    const INTERVAL: Duration = duration_constant!(30 minutes);

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    #[derive(Copy, Clone, Debug, PartialEq)]
    struct Request {}

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
        let id = Id::new("test");

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
            &id,
            7.68,
            start_time,
            end_time,
            required_duration,
            &prices,
            Request {},
        );
        let user_plan = MaybeUserPlan::new_none();
        let user_plan = user_plan.update_plan(&id, &prices, start_time, maybe_new_plan);

        let plan = user_plan.0.unwrap();
        let cost = plan.get_forecast_cost(&id, start_time, &prices).unwrap();
        assert_approx_eq!(f32, plan.get_kw(), 7.680);
        assert_eq!(plan.get_start_time(), expected_start_time);
        assert_eq!(plan.get_end_time(), expected_end_time);
        assert_approx_eq!(f32, cost, expected_cost);
    }
}
