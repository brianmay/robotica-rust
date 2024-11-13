//! This module contains the definition of the rules and rule sets used in the Amber system

use std::fmt::Debug;

use serde::Deserialize;

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

impl<T: PartialEq> PartialEq for RuleSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.rules == other.rules
    }
}
