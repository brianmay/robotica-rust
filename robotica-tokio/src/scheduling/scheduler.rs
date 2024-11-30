//! Create a schedule based on tags from classifier.
//!

use crate::conditions::ast::{BooleanExpr, FieldRef, Fields, GetValues, Reference, Scalar};
use chrono::Datelike;
use chrono::{NaiveDate, TimeZone};
use robotica_common::datetime::{
    convert_date_time_to_utc_or_default, week_day_to_string, DateTime, Time,
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use thiserror::Error;

/// An entry in the schedule.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Schedule {
    /// The date and time of the schedule.
    pub datetime: DateTime<chrono::Utc>,

    /// The name of the sequence to run.
    pub sequence_name: String,

    /// The options to use when running the sequence.
    pub options: HashSet<String>,
}

/// A sequence from the Config.
#[derive(Deserialize, Debug, Clone)]
pub struct Sequence {
    time: Time,
    options: Option<Vec<String>>,
}

/// The source schedule loaded from the config file.
#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(rename = "if")]
    if_cond: Option<Vec<BooleanExpr<Context>>>,
    today: Option<Vec<String>>,
    tomorrow: Option<Vec<String>>,
    sequences: HashMap<String, Sequence>,
}

impl Config {
    /// Retrieve the sequences from the config.
    #[must_use]
    pub const fn get_sequences(&self) -> &HashMap<String, Sequence> {
        &self.sequences
    }
}

#[derive(Debug)]
struct Context {
    today: HashSet<String>,
    tomorrow: HashSet<String>,
    day_of_week: String,
}

impl GetValues for Context {
    fn get_fields() -> Fields<Self> {
        let mut fields: Fields<Self> = Fields::default();
        fields.scalars.insert("day_of_week".to_string());
        fields.hash_sets.insert("today".to_string());
        fields.hash_sets.insert("tomorrow`".to_string());
        fields
    }

    fn get_scalar(&self, field: &Reference<Self>) -> Option<crate::conditions::ast::Scalar> {
        match field.get_name() {
            "day_of_week" => Some(Scalar::new_string(&self.day_of_week)),
            _ => None,
        }
    }

    fn get_hash_set(&self, field: &FieldRef<Self, HashSet<String>>) -> Option<&HashSet<String>> {
        match field.get_name() {
            "today" => Some(&self.today),
            "tomorrow" => Some(&self.tomorrow),
            _ => None,
        }
    }
}

/// An error loading the Config
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Error reading the file
    #[error("Error reading file {0}: {1}")]
    FileError(PathBuf, std::io::Error),

    /// Error reading the file
    #[error("Error parsing file {0}: {1}")]
    YamlError(PathBuf, serde_yml::Error),
}

/// Load the scheduler config from the given path.
///
/// # Errors
///
/// If the file cannot be read or parsed.
pub fn load_config(filename: &Path) -> Result<Vec<Config>, ConfigError> {
    let f = std::fs::File::open(filename)
        .map_err(|e| ConfigError::FileError(filename.to_path_buf(), e))?;

    let config: Vec<Config> =
        serde_yml::from_reader(f).map_err(|e| ConfigError::YamlError(filename.to_path_buf(), e))?;

    Ok(config)
}

fn is_condition_ok(
    config: &Config,
    date: NaiveDate,
    today: &HashSet<String>,
    tomorrow: &HashSet<String>,
) -> bool {
    config.if_cond.as_ref().map_or(true, |s_if| {
        s_if.iter().any(|s_if| {
            let context = Context {
                today: today.clone(),
                tomorrow: tomorrow.clone(),
                day_of_week: week_day_to_string(date.weekday()).to_lowercase(),
            };
            s_if.eval(&context).unwrap_or_else(|e| {
                tracing::error!("Error evaluating condition: {:?}", e);
                false
            })
        })
    })
}

fn is_tag_ok(c: &HashSet<String>, requirements: Option<&Vec<String>>) -> bool {
    #[allow(clippy::option_if_let_else)]
    if let Some(requirements) = requirements {
        requirements.iter().any(|r| c.contains(r))
    } else {
        true
    }
}
fn is_tags_ok(config: &Config, today: &HashSet<String>, tomorrow: &HashSet<String>) -> bool {
    is_tag_ok(today, config.today.as_ref()) && is_tag_ok(tomorrow, config.tomorrow.as_ref())
}

/// A scheduling error occurred
#[derive(Error, Debug)]
pub enum ScheduleError {}

/// Create a schedule for given date based on the given tags.
///
/// # Errors
///
/// Returns an error if a date time cannot be converted to UTC.
#[allow(clippy::implicit_hasher)]
pub fn get_schedule_with_config<T: TimeZone>(
    date: NaiveDate,
    today: &HashSet<String>,
    tomorrow: &HashSet<String>,
    config_list: &[Config],
    timezone: &T,
) -> Result<Vec<Schedule>, ScheduleError> {
    let schedule = config_list.iter().filter(|config| {
        is_condition_ok(config, date, today, tomorrow) && is_tags_ok(config, today, tomorrow)
    });

    let sequences: Result<Vec<Schedule>, ScheduleError> = schedule
        .fold(HashMap::new(), |mut acc, config| {
            acc.extend(config.sequences.clone());
            acc
        })
        .into_iter()
        .map(|(name, seq)| {
            let options = seq
                .options
                .as_ref()
                .map_or_else(HashSet::new, |o| o.iter().cloned().collect());
            let schedule = Schedule {
                datetime: convert_date_time_to_utc_or_default(date, seq.time, timezone),
                sequence_name: name,
                options,
            };
            Ok(schedule)
        })
        .collect();

    sequences.map(|s| {
        let mut s = s;
        s.sort_by_key(|s| s.datetime);
        s
    })
}

/// Error returned by `get_schedule`.
#[derive(Error, Debug)]
pub enum GetScheduleError {
    /// Error loading the config
    #[error("{0}")]
    ConfigError(#[from] ConfigError),

    /// Error processing the schedule.
    #[error("{0}")]
    ScheduleError(#[from] ScheduleError),
}

/// Create test schedule for testing.
///
/// # Panics
///
/// Panics if something goes wrong.
#[cfg(test)]
#[must_use]
pub fn create_test_config() -> Vec<Config> {
    #![allow(clippy::unwrap_used)]
    vec![Config {
        if_cond: None,
        today: None,
        tomorrow: None,
        sequences: HashMap::from([
            (
                "wake_up".to_string(),
                Sequence {
                    time: "08:30:00".parse().unwrap(),
                    options: None,
                },
            ),
            (
                "sleep".to_string(),
                Sequence {
                    time: "20:30:00".parse().unwrap(),
                    options: None,
                },
            ),
        ]),
    }]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use chrono::FixedOffset;
    use robotica_common::datetime::convert_date_time_to_utc;

    use crate::conditions::BooleanExprParser;

    use super::*;

    struct ExpectedResult {
        name: &'static str,
        time: &'static str,
        options: Vec<&'static str>,
    }

    #[test]
    fn test_load_config() {
        let config_list = load_config(Path::new("test/schedule.yaml")).unwrap();
        assert_eq!(config_list.len(), 3);
    }

    fn config() -> Vec<Config> {
        vec![
            Config {
                if_cond: None,
                today: None,
                tomorrow: None,
                sequences: HashMap::from([
                    (
                        "wake_up".to_string(),
                        Sequence {
                            time: "08:30:00".parse().unwrap(),
                            options: None,
                        },
                    ),
                    (
                        "sleep".to_string(),
                        Sequence {
                            time: "20:30:00".parse().unwrap(),
                            options: None,
                        },
                    ),
                ]),
            },
            Config {
                if_cond: None,
                today: Some(vec!["christmas".to_string()]),
                tomorrow: None,
                sequences: HashMap::from([(
                    "presents".to_string(),
                    Sequence {
                        time: "08:00:00".parse().unwrap(),
                        options: None,
                    },
                )]),
            },
            Config {
                if_cond: Some(vec![BooleanExprParser::new()
                    .parse(&Context::get_fields(), "'boxing' in today")
                    .unwrap()]),
                today: None,
                tomorrow: None,
                sequences: HashMap::from([(
                    "wake_up".to_string(),
                    Sequence {
                        time: "12:30:00".parse().unwrap(),
                        options: Some(vec!["late_wake_up".to_string()]),
                    },
                )]),
            },
        ]
    }

    #[test]
    fn test_every_day() {
        let config_list = config();

        let date = NaiveDate::from_ymd_opt(2018, 12, 24).unwrap();
        let today = ["today"];
        let today: HashSet<String> = today.iter().map(std::string::ToString::to_string).collect();
        let tomorrow: HashSet<String> = HashSet::from([]);

        let timezone = FixedOffset::east_opt(60 * 60 * 10).unwrap();

        let schedule =
            get_schedule_with_config(date, &today, &tomorrow, &config_list, &timezone).unwrap();

        let expected: Vec<ExpectedResult> = vec![
            ExpectedResult {
                name: "wake_up",
                time: "08:30:00",
                options: vec![],
            },
            ExpectedResult {
                name: "sleep",
                time: "20:30:00",
                options: vec![],
            },
        ];

        check_results(&schedule, &expected, date, timezone);
    }

    #[test]
    fn test_christmas() {
        let config_list = config();

        let date = NaiveDate::from_ymd_opt(2018, 12, 25).unwrap();
        let today = ["christmas"];
        let today: HashSet<String> = today.iter().map(std::string::ToString::to_string).collect();
        let tomorrow: HashSet<String> = HashSet::from([]);

        let timezone = FixedOffset::east_opt(60 * 60 * 10).unwrap();

        let schedule =
            get_schedule_with_config(date, &today, &tomorrow, &config_list, &timezone).unwrap();

        let expected: Vec<ExpectedResult> = vec![
            ExpectedResult {
                name: "presents",
                time: "08:00:00",
                options: vec![],
            },
            ExpectedResult {
                name: "wake_up",
                time: "08:30:00",
                options: vec![],
            },
            ExpectedResult {
                name: "sleep",
                time: "20:30:00",
                options: vec![],
            },
        ];

        check_results(&schedule, &expected, date, timezone);
    }

    #[test]
    fn test_boxing() {
        let config_list = config();

        let date = NaiveDate::from_ymd_opt(2018, 12, 26).unwrap();
        let today = ["boxing"];
        let today: HashSet<String> = today.iter().map(std::string::ToString::to_string).collect();
        let tomorrow: HashSet<String> = HashSet::from([]);

        let timezone = FixedOffset::east_opt(60 * 60 * 10).unwrap();

        let schedule =
            get_schedule_with_config(date, &today, &tomorrow, &config_list, &timezone).unwrap();

        let expected: Vec<ExpectedResult> = vec![
            ExpectedResult {
                name: "wake_up",
                time: "12:30:00",
                options: vec!["late_wake_up"],
            },
            ExpectedResult {
                name: "sleep",
                time: "20:30:00",
                options: vec![],
            },
        ];

        check_results(&schedule, &expected, date, timezone);
    }

    fn check_results(
        schedule: &[Schedule],
        expected: &[ExpectedResult],
        date: NaiveDate,
        timezone: FixedOffset,
    ) {
        assert!(
            schedule.len() == expected.len(),
            "{:?} != {:?} in {:?}",
            schedule.len(),
            expected.len(),
            schedule
        );

        for (i, s) in schedule.iter().enumerate() {
            let e = &expected[i];

            let expected_time = e.time.parse::<Time>().unwrap();
            let expected_datetime =
                convert_date_time_to_utc(date, expected_time, &timezone).unwrap();
            let expected_options = e
                .options
                .iter()
                .map(std::string::ToString::to_string)
                .collect();

            assert!(
                s.sequence_name == e.name,
                "{:?} != {:?} in {:?}",
                s.sequence_name,
                e.name,
                s
            );
            assert!(
                s.datetime == expected_datetime,
                "{:?} != {:?} in {:?}",
                s.datetime,
                expected_datetime,
                s
            );
            assert!(
                s.options == expected_options,
                "{:?} != {:?} in {:?}",
                s.options,
                expected_options,
                s
            );
        }
    }
}
