//! The combined plan and rules amber state

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::robotica::amber::rules::Context;

use super::{rules::RuleSet, user_plan::MaybeUserPlan};

/// The combined plan and rules amber state
#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct State<R> {
    /// The time of the state
    time: DateTime<Utc>,

    /// The user plan
    plan: MaybeUserPlan<R>,

    /// The rules
    rules: RuleSet<R>,

    /// The rules context
    rules_context: Context,

    /// The rules result
    rules_result: Option<R>,

    /// The result of the state
    result: R,
}

impl<R> State<R> {
    /// Get the new plan
    #[must_use]
    pub const fn get_plan(&self) -> &MaybeUserPlan<R> {
        &self.plan
    }

    /// Get the rules
    #[must_use]
    pub const fn get_rules(&self) -> &RuleSet<R> {
        &self.rules
    }

    /// Get the rules context
    #[must_use]
    pub const fn get_rules_context(&self) -> &Context {
        &self.rules_context
    }

    /// Get the rules result
    #[must_use]
    pub const fn get_rules_result(&self) -> Option<&R> {
        self.rules_result.as_ref()
    }

    /// Get the result of the state
    pub const fn get_result(&self) -> &R {
        &self.result
    }
}
