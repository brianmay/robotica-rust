//! This module contains the definition of the rules and rule sets used in the Amber system

use std::fmt::Debug;

use serde::Deserialize;

#[derive(Debug, PartialEq, Clone, Deserialize)]
/// The context for the rules
pub struct Context {
    /// The number of days since the epoch
    days_since_epoch: i32,
    /// The hour of the day
    hour: i32,
    /// The day of the week
    day_of_week: String,
    /// The current price of electricity
    current_price: f32,
    /// The weighted price of electricity
    weighted_price: f32,
    /// Whether the device is currently on
    is_on: bool,
}

impl Context {
    /// Get days since epoch
    #[must_use]
    pub const fn get_days_since_epoch(&self) -> i32 {
        self.days_since_epoch
    }

    /// Get hour
    #[must_use]
    pub const fn get_hour(&self) -> i32 {
        self.hour
    }

    /// Get day of week
    #[must_use]
    pub fn get_day_of_week(&self) -> &str {
        &self.day_of_week
    }

    /// Get current price
    #[must_use]
    pub const fn get_current_price(&self) -> f32 {
        self.current_price
    }

    /// Get weighted price
    #[must_use]
    pub const fn get_weighted_price(&self) -> f32 {
        self.weighted_price
    }

    /// Get `is_on`
    #[must_use]
    pub const fn get_is_on(&self) -> bool {
        self.is_on
    }
}

/// A rule
#[derive(Deserialize)]
pub struct Rule<T> {
    condition: String,
    result: T,
}

impl<T: Clone> Clone for Rule<T> {
    fn clone(&self) -> Self {
        Self {
            condition: self.condition.clone(),
            result: self.result.clone(),
        }
    }
}

impl<T: Debug> Debug for Rule<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rule")
            .field("condition", &self.condition)
            .field("result", &self.result)
            .finish()
    }
}
impl<T: PartialEq> PartialEq for Rule<T> {
    fn eq(&self, other: &Self) -> bool {
        self.condition == other.condition && self.result == other.result
    }
}

impl<T> Rule<T> {
    /// Get the condition
    #[must_use]
    pub fn get_condition(&self) -> &str {
        &self.condition
    }

    /// Get the result
    #[must_use]
    pub const fn get_result(&self) -> &T {
        &self.result
    }
}

/// A rule set
#[derive(Deserialize)]
pub struct RuleSet<T> {
    rules: Vec<Rule<T>>,
}

impl<T: Clone> Clone for RuleSet<T> {
    fn clone(&self) -> Self {
        Self {
            rules: self.rules.clone(),
        }
    }
}

impl<T: Debug> Debug for RuleSet<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuleSet")
            .field("rules", &self.rules)
            .finish()
    }
}

impl<T> RuleSet<T> {
    /// Get the rules
    #[must_use]
    pub fn get_rules(&self) -> &[Rule<T>] {
        &self.rules
    }
}

impl<T: PartialEq> PartialEq for RuleSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.rules == other.rules
    }
}
