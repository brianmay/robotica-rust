//! Classification of a date.
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use chrono::Datelike;
use evalexpr::{
    build_operator_tree, ContextWithMutableFunctions, ContextWithMutableVariables,
    DefaultNumericTypes, Function, HashMapContext, Node, Value,
};
use serde::{Deserialize, Deserializer};
use thiserror::Error;

use robotica_common::datetime::{num_days_from_ce, week_day_to_string, Date, Weekday};

/// A compiled evalexpr condition.
#[derive(Debug, Clone)]
pub struct Condition(Node);

impl<'de> Deserialize<'de> for Condition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        build_operator_tree::<DefaultNumericTypes>(&s)
            .map(Condition)
            .map_err(|e| serde::de::Error::custom(format!("Error parsing condition: {e}")))
    }
}

fn build_context(
    days_since_epoch: i32,
    day_of_week: &str,
    classifications: &HashSet<String>,
) -> HashMapContext<DefaultNumericTypes> {
    let classifications = classifications.clone();
    let mut ctx = HashMapContext::new();
    ctx.set_value(
        "days_since_epoch".into(),
        Value::Int(i64::from(days_since_epoch)),
    )
    .ok();
    ctx.set_value("day_of_week".into(), Value::String(day_of_week.to_string()))
        .ok();
    ctx.set_function(
        "classifications".to_string(),
        Function::new(move |arg| {
            let tag = arg.as_string()?;
            Ok(Value::Boolean(classifications.contains(&tag)))
        }),
    )
    .ok();
    ctx
}

/// Classify a date into tags.
#[derive(Deserialize, Debug)]
pub struct Config {
    date: Option<Date>,
    start: Option<Date>,
    stop: Option<Date>,
    week_day: Option<bool>,
    day_of_week: Option<Weekday>,
    #[serde(rename = "if")]
    if_cond: Option<Vec<Condition>>,
    if_set: Option<Vec<String>>,
    if_not_set: Option<Vec<String>>,
    add: Option<Vec<String>>,
    delete: Option<Vec<String>>,
}

/// An error loading the Config
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Error reading the file
    #[error("Error reading file {0}: {1}")]
    FileError(PathBuf, std::io::Error),

    /// Error reading the file
    #[error("Error parsing file {0}: {1}")]
    YamlError(PathBuf, serde_yaml_ng::Error),
}

/// Load the classifier config from the given path.
///
/// # Errors
///
/// If the file cannot be read or parsed.
pub fn load_config(filename: &Path) -> Result<Vec<Config>, ConfigError> {
    let f = std::fs::File::open(filename)
        .map_err(|e| ConfigError::FileError(filename.to_path_buf(), e))?;

    let config: Vec<Config> = serde_yaml_ng::from_reader(f)
        .map_err(|e| ConfigError::YamlError(filename.to_path_buf(), e))?;

    Ok(config)
}

/// Classify a date.
///
/// # Errors
///
/// Returns an error if the environment variable `CLASSIFICATIONS_FILE` is not set or if the file
/// cannot be read or parsed.
#[must_use]
pub fn classify_date_with_config(date: &Date, config: &Vec<Config>) -> HashSet<String> {
    let mut tags = HashSet::new();

    for c in config {
        if let Some(c_date) = c.date {
            if c_date != *date {
                continue;
            }
        }
        if let Some(start) = c.start {
            if *date < start {
                continue;
            }
        }
        if let Some(stop) = c.stop {
            if *date > stop {
                continue;
            }
        }
        let weekday = date.weekday();
        if let Some(weekday_wanted) = c.week_day {
            let is_weekday = !(weekday == chrono::Weekday::Sat || weekday == chrono::Weekday::Sun);

            if is_weekday != weekday_wanted {
                continue;
            }
        }
        if let Some(day_of_week) = c.day_of_week {
            if weekday != day_of_week {
                continue;
            }
        }
        if let Some(if_cond) = &c.if_cond {
            let ctx = build_context(
                num_days_from_ce(date),
                &week_day_to_string(date.weekday()).to_lowercase(),
                &tags,
            );
            if !if_cond.iter().any(|cond| {
                cond.0.eval_boolean_with_context(&ctx).unwrap_or_else(|e| {
                    tracing::error!("Error evaluating condition: {}", e);
                    false
                })
            }) {
                continue;
            }
        }
        if let Some(if_set) = &c.if_set {
            if !tags.iter().any(|t| if_set.contains(t)) {
                continue;
            }
        }
        if let Some(if_not_set) = &c.if_not_set {
            if tags.iter().any(|t| if_not_set.contains(t)) {
                continue;
            }
        }
        if let Some(add) = &c.add {
            tags.extend(add.iter().cloned());
        }
        if let Some(delete) = &c.delete {
            tags.retain(|t| !delete.contains(t));
        }
    }

    tags
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use std::str::FromStr;

    use super::*;

    struct Test {
        date: Date,
        includes: Vec<&'static str>,
        excludes: Vec<&'static str>,
    }

    #[test]
    fn test_load_test_file() {
        let config = load_config(Path::new("test/classifications.yaml")).unwrap();

        let tests: Vec<Test> = vec![
            Test {
                date: Date::from_str("2018-12-22").unwrap(),
                includes: vec!["saturday", "good_day", "bad_day"],
                excludes: vec!["weekday", "sunday", "christmas"],
            },
            Test {
                date: Date::from_str("2018-12-23").unwrap(),
                includes: vec!["sunday", "good_day", "bad_day"],
                excludes: vec!["weekday", "saturday", "christmas"],
            },
            Test {
                date: Date::from_str("2018-12-24").unwrap(),
                includes: vec!["weekday", "bad_day"],
                excludes: vec!["saturday", "sunday", "good_day", "christmas"],
            },
            Test {
                date: Date::from_str("2018-12-25").unwrap(),
                includes: vec!["weekday", "good_day", "christmas"],
                excludes: vec!["saturday", "sunday", "bad_day"],
            },
            Test {
                date: Date::from_str("2018-12-26").unwrap(),
                includes: vec!["weekday", "bad_day"],
                excludes: vec!["saturday", "sunday", "good_day", "christmas"],
            },
            Test {
                date: Date::from_str("2018-12-27").unwrap(),
                includes: vec!["weekday", "bad_day"],
                excludes: vec!["saturday", "sunday", "good_day", "christmas"],
            },
            Test {
                date: Date::from_str("2018-12-28").unwrap(),
                includes: vec!["weekday", "bad_day"],
                excludes: vec!["saturday", "sunday", "good_day", "christmas"],
            },
        ];

        for test in tests {
            let tags = classify_date_with_config(&test.date, &config);
            for include in test.includes {
                assert!(
                    tags.contains(include),
                    "{} missing tag {include}",
                    test.date
                );
            }
            for exclude in test.excludes {
                assert!(
                    !tags.contains(exclude),
                    "{} contains tag: {exclude}",
                    test.date
                );
            }
        }
    }

    #[test]
    fn test_classify() {
        let config = vec![
            Config {
                date: None,
                start: Some(Date::from_ymd_opt(2020, 1, 1).unwrap()),
                stop: Some(Date::from_ymd_opt(2020, 12, 31).unwrap()),
                week_day: Some(true),
                day_of_week: None,
                if_cond: None,
                if_set: None,
                if_not_set: None,
                add: Some(vec!["weekday".to_string()]),
                delete: None,
            },
            Config {
                date: None,
                start: Some(Date::from_ymd_opt(2020, 1, 1).unwrap()),
                stop: Some(Date::from_ymd_opt(2020, 12, 31).unwrap()),
                week_day: None,
                day_of_week: Some(chrono::Weekday::Mon),
                if_cond: None,
                if_set: None,
                if_not_set: None,
                add: Some(vec!["monday".to_string()]),
                delete: None,
            },
        ];

        let date = Date::from_ymd_opt(2019, 1, 7).unwrap();
        let tags = classify_date_with_config(&date, &config);
        assert_eq!(tags, HashSet::from([]));

        let date = Date::from_ymd_opt(2020, 1, 1).unwrap();
        let tags = classify_date_with_config(&date, &config);
        assert_eq!(tags, HashSet::from(["weekday".to_string()]));

        let date = Date::from_ymd_opt(2020, 1, 4).unwrap();
        let tags = classify_date_with_config(&date, &config);
        assert_eq!(tags, HashSet::from([]));

        let date = Date::from_ymd_opt(2020, 1, 6).unwrap();
        let tags = classify_date_with_config(&date, &config);
        assert_eq!(
            tags,
            HashSet::from(["weekday".to_string(), "monday".to_string()])
        );

        let date = Date::from_ymd_opt(2021, 1, 4).unwrap();
        let tags = classify_date_with_config(&date, &config);
        assert_eq!(tags, HashSet::from([]));
    }

    #[test]
    fn test_cond() {
        let conditions = vec![
            Condition(build_operator_tree::<DefaultNumericTypes>("10 == 11").unwrap()),
            Condition(
                build_operator_tree::<DefaultNumericTypes>(r#"classifications("classifications")"#)
                    .unwrap(),
            ),
        ];

        let config = vec![Config {
            date: None,
            start: None,
            stop: None,
            week_day: None,
            day_of_week: None,
            if_cond: Some(conditions),
            if_set: None,
            if_not_set: None,
            add: Some(vec!["true".to_string()]),
            delete: None,
        }];

        let date = Date::from_ymd_opt(2020, 1, 1).unwrap();
        let tags = classify_date_with_config(&date, &config);
        assert_eq!(tags, HashSet::from([]));
    }
}
