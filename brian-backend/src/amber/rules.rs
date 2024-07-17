use std::{collections::HashSet, fmt::Debug};

use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use robotica_backend::conditions::ast::{
    BooleanExpr, FieldRef, Fields, GetValues, Reference, Scalar,
};
use robotica_common::datetime::{num_days_from_ce, week_day_to_string};
use serde::{Deserialize, Serialize};

use super::Prices;

#[derive(Debug, PartialEq)]
pub struct Context {
    days_since_epoch: i32,
    hour: i32,
    day_of_week: String,
    current_price: f32,
    weighted_price: f32,
    is_on: bool,
}

impl Context {
    pub fn new<TZ: TimeZone>(
        prices: &Prices,
        now: DateTime<Utc>,
        is_on: bool,
        timezone: &TZ,
    ) -> Self {
        let local = now.with_timezone(timezone);
        let day_of_week = week_day_to_string(local.weekday());
        let hour = i32::try_from(local.hour()).unwrap_or(-1);
        let date = local.date_naive();

        let current_price = prices.current(&now).map_or(100.0, |f| f.per_kwh);
        let weighted_price = prices.get_weighted_price(now).unwrap_or(100.0);

        Self {
            days_since_epoch: num_days_from_ce(&date),
            day_of_week,
            hour,
            current_price,
            weighted_price,
            is_on,
        }
    }
}

impl GetValues for Context {
    fn get_fields() -> Fields<Self> {
        let mut fields: Fields<Self> = Fields::default();
        fields.scalars.insert("days_since_epoch".to_string());
        fields.scalars.insert("day_of_week".to_string());
        fields.scalars.insert("hour".to_string());
        fields.scalars.insert("current_price".to_string());
        fields.scalars.insert("weighted_price".to_string());
        fields.scalars.insert("is_on".to_string());
        fields
    }

    fn get_scalar(&self, field: &Reference<Self>) -> Option<Scalar> {
        match field.get_name() {
            "days_since_epoch" => Some(Scalar::Integer(self.days_since_epoch)),
            "day_of_week" => Some(Scalar::String(self.day_of_week.clone())),
            "hour" => Some(Scalar::Integer(self.hour)),
            "current_price" => Some(Scalar::Float(self.current_price)),
            "weighted_price" => Some(Scalar::Float(self.weighted_price)),
            "is_on" => Some(Scalar::Boolean(self.is_on)),
            _ => None,
        }
    }

    fn get_hash_set(&self, _field: &FieldRef<Self, HashSet<String>>) -> Option<&HashSet<String>> {
        None
    }
}

#[derive(Deserialize, Serialize)]
pub struct Rule<T> {
    condition: BooleanExpr<Context>,
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
    #[cfg(test)]
    pub const fn new(condition: BooleanExpr<Context>, result: T) -> Self {
        Self { condition, result }
    }
}

#[derive(Deserialize, Serialize)]
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

impl<T> RuleSet<T> {
    pub fn new(rules: Vec<Rule<T>>) -> Self {
        Self { rules }
    }

    pub fn apply<TZ: TimeZone>(
        &self,
        prices: &Prices,
        now: DateTime<Utc>,
        is_on: bool,
        timezone: &TZ,
    ) -> Option<&T> {
        let context = Context::new(prices, now, is_on, timezone);

        self.rules
            .iter()
            .find(|rule| {
                rule.condition.eval(&context).unwrap_or_else(|e| {
                    tracing::error!("Error evaluating rule: {:?}", e);
                    false
                })
            })
            .map(|rule| &rule.result)
    }
}
