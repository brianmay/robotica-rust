//! Classification of a date.
use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
};

use crate::scheduling::conditions;
use chrono::Datelike;
use field_ref::field_ref_of;
use serde::{Deserialize, Deserializer};
use thiserror::Error;

use robotica_common::datetime::{num_days_from_ce, Date, Weekday};

use super::ast::{Boolean, Fields, Reference};

#[derive(Debug)]
struct Context {
    days_since_epoch: i32,
    classifications: HashSet<String>,
    day_of_week: String,
}

fn get_fields() -> Fields<Context> {
    let mut fields: Fields<Context> = Fields {
        any: HashMap::new(),
        sets: HashMap::new(),
    };
    fields.any.insert(
        "days_since_epoch".to_string(),
        Reference::Int(field_ref_of!(Context => days_since_epoch)),
    );
    fields.any.insert(
        "day_of_week".to_string(),
        Reference::String(field_ref_of!(Context => day_of_week)),
    );
    fields.sets.insert(
        "classifications".to_string(),
        field_ref_of!(Context => classifications),
    );
    fields
}

impl<'de> Deserialize<'de> for Boolean<Context> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = get_fields();
        let s: String = Deserialize::deserialize(deserializer)?;
        let expr = conditions::BooleanParser::new()
            .parse(&fields, &s)
            .map_err(|e| {
                serde::de::Error::custom(format!("Error parsing classifier config: {e}"))
            })?;
        Ok(expr)
    }
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
    if_cond: Option<Vec<Boolean<Context>>>,
    if_set: Option<Vec<String>>,
    if_not_set: Option<Vec<String>>,
    add: Option<Vec<String>>,
    delete: Option<Vec<String>>,
}

/// An error loading the Config
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Environment variable not set
    #[error("Environment variable missing: {0}")]
    VarError(String),

    /// Error reading the file
    #[error("Error reading file {0}: {1}")]
    FileError(PathBuf, std::io::Error),

    /// Error reading the file
    #[error("Error parsing file {0}: {1}")]
    YamlError(PathBuf, serde_yaml::Error),
}

/// Load the classifier config from the given path.
///
/// # Errors
///
/// If the file cannot be read or parsed.
pub fn load_config(filename: &Path) -> Result<Vec<Config>, ConfigError> {
    let f = std::fs::File::open(filename)
        .map_err(|e| ConfigError::FileError(filename.to_path_buf(), e))?;

    let config: Vec<Config> = serde_yaml::from_reader(f)
        .map_err(|e| ConfigError::YamlError(filename.to_path_buf(), e))?;

    Ok(config)
}

/// Load the classification config from the environment variable `CLASSIFICATIONS_FILE`.
///
/// # Errors
///
/// Returns an error if the environment variable `CLASSIFICATIONS_FILE` is not set or if the file cannot be read.
pub fn load_config_from_default_file() -> Result<Vec<Config>, ConfigError> {
    let env_name = "CLASSIFICATIONS_FILE";
    let filename = env::var(env_name).map_err(|_| ConfigError::VarError(env_name.to_string()))?;
    load_config(Path::new(&filename))
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
            let context = Context {
                days_since_epoch: num_days_from_ce(date),
                classifications: tags.clone(),
                day_of_week: date.weekday().to_string().to_lowercase(),
            };
            if !if_cond.iter().any(|c| c.eval(&context)) {
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

/// Classify a date.
///
/// # Errors
///
/// Returns an error if the environment variable `CLASSIFICATIONS_FILE` is not set or if the file
/// cannot be read or parsed.
pub fn classify_date(date: &Date) -> Result<HashSet<String>, ConfigError> {
    let config = load_config_from_default_file()?;
    Ok(classify_date_with_config(date, &config))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use std::str::FromStr;

    use crate::scheduling::ast::{Condition, ConditionOpcode, Expr};

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
            Boolean::Cond(Condition::Op(
                Box::new(Expr::Number(10)),
                ConditionOpcode::Eq,
                Box::new(Expr::Number(11)),
            )),
            Boolean::Cond(Condition::In(
                "classifications".to_string(),
                field_ref_of!(Context => classifications),
            )),
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
