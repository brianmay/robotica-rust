//! Create a schedule based on tags from classifier.
//!
use super::{
    ast::{Boolean, Fields},
    conditions,
    types::{convert_date_time_to_utc, Date, DateTime, DateTimeError, Time},
};
use chrono::TimeZone;
use field_ref::field_ref_of;
use serde::{Deserialize, Deserializer};
use std::{
    collections::{HashMap, HashSet},
    env,
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
    pub name: String,

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
    if_cond: Option<Vec<Boolean<Context>>>,
    today: Option<Vec<String>>,
    tomorrow: Option<Vec<String>>,
    sequences: HashMap<String, Sequence>,
}

#[derive(Debug)]
struct Context {
    today: HashSet<String>,
    tomorrow: HashSet<String>,
}

fn get_fields() -> Fields<Context> {
    let mut fields: Fields<Context> = Fields {
        any: HashMap::new(),
        sets: HashMap::new(),
    };

    fields
        .sets
        .insert("today".to_string(), field_ref_of!(Context => today));
    fields
        .sets
        .insert("tomorrow".to_string(), field_ref_of!(Context => tomorrow));
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

/// Load the scheduler config from the given path.
///
/// # Errors
///
/// If the file cannot be read or parsed.
pub fn load_config(filename: &Path) -> Result<Vec<Config>, ConfigError> {
    let f = std::fs::File::open(&filename)
        .map_err(|e| ConfigError::FileError(filename.to_path_buf(), e))?;

    let config: Vec<Config> = serde_yaml::from_reader(f)
        .map_err(|e| ConfigError::YamlError(filename.to_path_buf(), e))?;

    Ok(config)
}

/// Load the classification config from the environment variable `SCHEDULE_FILE`.
///
/// # Errors
///
/// Returns an error if the environment variable `SCHEDULE_FILE` is not set or if the file cannot be read.
pub fn load_config_from_default_file() -> Result<Vec<Config>, ConfigError> {
    let env_name = "SCHEDULE_FILE";
    let filename = env::var(env_name).map_err(|_| ConfigError::VarError(env_name.to_string()))?;
    load_config(Path::new(&filename))
}

fn is_condition_ok(config: &Config, today: &HashSet<String>, tomorrow: &HashSet<String>) -> bool {
    config.if_cond.as_ref().map_or(true, |s_if| {
        s_if.iter().any(|s_if| {
            let context = Context {
                today: today.clone(),
                tomorrow: tomorrow.clone(),
            };
            s_if.eval(&context)
        })
    })
}

fn is_tag_ok(c: &HashSet<String>, requirements: &Option<Vec<String>>) -> bool {
    #[allow(clippy::option_if_let_else)]
    if let Some(requirements) = requirements {
        requirements.iter().any(|r| c.contains(r))
    } else {
        true
    }
}
fn is_tags_ok(config: &Config, today: &HashSet<String>, tomorrow: &HashSet<String>) -> bool {
    is_tag_ok(today, &config.today) && is_tag_ok(tomorrow, &config.tomorrow)
}

/// A scheduling error occurred
#[derive(Error, Debug)]
pub enum ScheduleError<T: TimeZone> {
    /// Error translating the date time to UTC.
    #[error("Error translating date: {0}")]
    DateTimeError(#[from] DateTimeError<T>),
}

/// Create a schedule for given date based on the given tags.
///
/// # Errors
///
/// Returns an error if a date time cannot be converted to UTC.
#[allow(clippy::implicit_hasher)]
pub fn get_schedule_with_config<T: TimeZone>(
    date: Date,
    today: &HashSet<String>,
    tomorrow: &HashSet<String>,
    config_list: &[Config],
    timezone: &T,
) -> Result<Vec<Schedule>, ScheduleError<T>> {
    let schedule = config_list.iter().filter(|config| {
        is_condition_ok(config, today, tomorrow) && is_tags_ok(config, today, tomorrow)
    });

    let sequences: Result<Vec<Schedule>, ScheduleError<T>> = schedule
        .fold(HashMap::new(), |mut acc, config| {
            acc.extend(config.sequences.clone());
            acc
        })
        .into_iter()
        .map(|(name, seq)| {
            let options = seq
                .options
                .as_ref()
                .map_or(HashSet::new(), |o| o.iter().cloned().collect());
            let schedule = Schedule {
                datetime: convert_date_time_to_utc(date, seq.time, timezone)?,
                name,
                options,
            };
            Ok(schedule)
        })
        .collect();

    sequences.map(|s| {
        let mut s = s;
        s.sort_by_key(|s| s.datetime.clone());
        s
    })
}

/// Error returned by `get_schedule`.
#[derive(Error, Debug)]
pub enum GetScheduleError<T: TimeZone> {
    /// Error loading the config
    #[error("{0}")]
    ConfigError(#[from] ConfigError),

    /// Error processing the schedule.
    #[error("{0}")]
    ScheduleError(#[from] ScheduleError<T>),
}

/// Create a schedule for given date based on the given tags.
///
/// # Errors
///
/// Returns an error if config file cannot be loaded or error processing schedule.
#[allow(clippy::implicit_hasher)]
pub fn get_schedule<T: TimeZone>(
    date: Date,
    today: &HashSet<String>,
    tomorrow: &HashSet<String>,
    timezone: &T,
) -> Result<Vec<Schedule>, GetScheduleError<T>> {
    let config_list = load_config_from_default_file()?;
    let schedule = get_schedule_with_config(date, today, tomorrow, &config_list, timezone)?;
    Ok(schedule)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use chrono::FixedOffset;

    use crate::scheduling::conditions::BooleanParser;

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
                if_cond: Some(vec![BooleanParser::new()
                    .parse(&get_fields(), "'boxing' in today")
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

        let date = Date::from_ymd(2018, 12, 24);
        let today = vec!["today"];
        let today: HashSet<String> = today.iter().map(std::string::ToString::to_string).collect();
        let tomorrow: HashSet<String> = HashSet::from([]);

        let timezone = FixedOffset::east(60 * 60 * 10);

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

        let date = Date::from_ymd(2018, 12, 25);
        let today = vec!["christmas"];
        let today: HashSet<String> = today.iter().map(std::string::ToString::to_string).collect();
        let tomorrow: HashSet<String> = HashSet::from([]);

        let timezone = FixedOffset::east(60 * 60 * 10);

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

        let date = Date::from_ymd(2018, 12, 26);
        let today = vec!["boxing"];
        let today: HashSet<String> = today.iter().map(std::string::ToString::to_string).collect();
        let tomorrow: HashSet<String> = HashSet::from([]);

        let timezone = FixedOffset::east(60 * 60 * 10);

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
        date: Date,
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

            assert!(s.name == e.name, "{:?} != {:?} in {:?}", s.name, e.name, s);
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
