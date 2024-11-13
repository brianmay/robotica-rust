//! A plan with user data

use chrono::{DateTime, TimeDelta, Utc};
use serde::Deserialize;

use super::plan::Plan;

/// An plan with user data
#[derive(Deserialize, Debug, PartialEq, Clone)]
pub struct UserPlan<R> {
    plan: Plan,
    request: R,
    cost: f32,
}

impl<R> UserPlan<R> {
    /// Get the start time of the plan
    pub const fn get_start_time(&self) -> DateTime<Utc> {
        self.plan.get_start_time()
    }

    /// Get the end time of the plan
    pub const fn get_end_time(&self) -> DateTime<Utc> {
        self.plan.get_end_time()
    }

    /// Get the time delta of the plan
    #[allow(dead_code)]
    pub fn get_timedelta(&self) -> TimeDelta {
        self.plan.get_timedelta()
    }

    /// Get the time left in the plan
    pub fn get_time_left(&self, now: DateTime<Utc>) -> TimeDelta {
        self.plan.get_time_left(now)
    }

    /// Check if the plan is current
    pub fn is_current(&self, now: DateTime<Utc>) -> bool {
        self.plan.is_current(now)
    }

    /// Get average cost of the plan per hour
    pub fn get_average_cost_per_hour(&self) -> f32 {
        let duration = self.plan.get_timedelta();
        #[allow(clippy::cast_precision_loss)]
        let duration = duration.num_seconds() as f32 / 3600.0;
        self.cost / duration
    }

    /// Get the total cost of the plan
    pub const fn get_total_cost(&self) -> f32 {
        self.cost
    }

    /// Get the kw of the plan
    pub const fn get_kw(&self) -> f32 {
        self.plan.get_kw()
    }

    /// Get the request of the plan
    pub const fn get_request(&self) -> &R {
        &self.request
    }
}

/// An optional plan with user data
#[allow(clippy::module_name_repetitions)]
#[derive(Deserialize, Debug, PartialEq, Clone)]
pub struct MaybeUserPlan<R>(Option<UserPlan<R>>);

impl<R> MaybeUserPlan<R> {
    /// Get the plan
    pub const fn get(&self) -> Option<&UserPlan<R>> {
        self.0.as_ref()
    }

    // pub fn is_current(&self, now: DateTime<Utc>) -> bool {
    //     self.get_plan().map_or(false, |plan| plan.is_current(now))
    // }

    /// Get the average cost of the plan per hour
    pub fn get_average_cost_per_hour(&self) -> Option<f32> {
        self.0.as_ref().map(UserPlan::get_average_cost_per_hour)
    }

    /// Get the total cost of the plan
    pub fn get_total_cost(&self) -> Option<f32> {
        self.0.as_ref().map(UserPlan::get_total_cost)
    }
}
