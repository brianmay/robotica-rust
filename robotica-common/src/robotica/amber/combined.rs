//! The combined plan and rules amber state

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{rules::RuleSet, user_plan::MaybeUserPlan};

/// The combined plan and rules amber state
#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct State<R> {
    /// The time of the state
    time: DateTime<Utc>,

    /// The plan request
    plan_request: Option<R>,

    /// The rules request
    rules_request: Option<R>,

    /// The result of the state
    result: R,

    /// The user plan
    plan: MaybeUserPlan<R>,

    /// The rules
    // #[serde(with = "robotica_common::datetime::with_option_time_delta")]
    // estimated_time_to_plan: Option<TimeDelta>,
    rules: RuleSet<R>,
}

impl<R> State<R> {
    /// Get the new plan
    #[allow(dead_code)]
    pub const fn get_plan(&self) -> &MaybeUserPlan<R> {
        &self.plan
    }

    // pub const fn get_rules(&self) -> &RuleSet<R> {
    //     &self.rules
    // }

    // pub const fn get_estimated_time_to_plan(&self) -> Option<TimeDelta> {
    //     self.estimated_time_to_plan
    // }
}

impl<R> State<R> {
    /// Get the result of the state
    pub const fn get_result(&self) -> &R {
        &self.result
    }
}
