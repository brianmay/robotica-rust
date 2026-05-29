use std::{fmt::Debug, str::FromStr};

use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use evalexpr::{
    build_operator_tree, ContextWithMutableVariables, DefaultNumericTypes, EvalexprError,
    HashMapContext, Node, Value,
};
use robotica_common::datetime::{num_days_from_ce, week_day_to_string};
use robotica_common::robotica::entities::Id;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::Prices;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
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
        id: &Id,
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
        let weighted_price = prices.get_weighted_price(id, now).unwrap_or(100.0);

        Self {
            days_since_epoch: num_days_from_ce(&date),
            day_of_week,
            hour,
            current_price,
            weighted_price,
            is_on,
        }
    }

    fn to_evalexpr_context(&self) -> HashMapContext<DefaultNumericTypes> {
        let mut ctx = HashMapContext::new();
        ctx.set_value(
            "days_since_epoch".into(),
            Value::Int(i64::from(self.days_since_epoch)),
        )
        .ok();
        ctx.set_value(
            "day_of_week".into(),
            Value::String(self.day_of_week.clone()),
        )
        .ok();
        ctx.set_value("hour".into(), Value::Int(i64::from(self.hour)))
            .ok();
        ctx.set_value(
            "current_price".into(),
            Value::Float(f64::from(self.current_price)),
        )
        .ok();
        ctx.set_value(
            "weighted_price".into(),
            Value::Float(f64::from(self.weighted_price)),
        )
        .ok();
        ctx.set_value("is_on".into(), Value::Boolean(self.is_on))
            .ok();
        ctx
    }
}

/// A compiled evalexpr condition.
#[derive(Clone)]
pub struct Condition {
    source: String,
    node: Node,
}

#[allow(clippy::use_self, clippy::missing_fields_in_debug)]
impl Debug for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Condition")
            .field("source", &self.source)
            .finish()
    }
}

impl PartialEq for Condition {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Serialize for Condition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.source)
    }
}

impl FromStr for Condition {
    type Err = EvalexprError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        build_operator_tree::<DefaultNumericTypes>(s).map(|node| Self {
            source: s.to_string(),
            node,
        })
    }
}

impl<'de> Deserialize<'de> for Condition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let source: String = Deserialize::deserialize(deserializer)?;
        build_operator_tree::<DefaultNumericTypes>(&source)
            .map(|node| Self { source, node })
            .map_err(|e| serde::de::Error::custom(format!("Error parsing condition: {e}")))
    }
}

#[derive(Serialize, Deserialize)]
pub struct Rule<T> {
    condition: Condition,
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
    pub fn new(condition: Condition, result: T) -> Self {
        Self { condition, result }
    }
}

#[derive(Serialize, Deserialize)]
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
    pub const fn new(rules: Vec<Rule<T>>) -> Self {
        Self { rules }
    }

    pub fn apply(&self, context: &Context) -> Option<&T> {
        let ctx = context.to_evalexpr_context();
        self.rules
            .iter()
            .find(|rule| {
                rule.condition
                    .node
                    .eval_boolean_with_context(&ctx)
                    .unwrap_or_else(|e| {
                        tracing::error!("Error evaluating rule: {:?}", e);
                        false
                    })
            })
            .map(|rule| &rule.result)
    }
}
