//! Assemble schedule into sequences
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{NaiveDate, Utc};
use field_ref::field_ref_of;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use robotica_common::{
    datetime::{duration, DateTime},
    mqtt::{QoS, Retain},
    robotica::tasks::{Payload, Task},
    scheduler::{Importance, Mark, Status},
};

use super::{
    ast::{Boolean, Fields},
    conditions,
    scheduler::{self},
};

/// A task in a config sequence.
#[derive(Deserialize, Debug, Clone)]
pub struct ConfigTask {
    /// The title of the task.
    pub title: String,

    /// The payload of the task.
    #[serde(flatten)]
    pub payload: Option<Payload>,

    /// The qos to be used when sending the message.
    qos: Option<u8>,

    /// The retain flag to be used when sending the message.
    retain: Option<Retain>,

    /// The topics this task will send to.
    topics: Vec<String>,
}

/// The source schedule loaded from the config file.
#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    /// The title of the sequence.
    pub title: String,

    /// The id of the step.
    pub id: Option<String>,

    /// The importance of the sequence.
    #[serde(default)]
    pub importance: Importance,

    /// The conditions that must be true before this is scheduled.
    #[serde(rename = "if")]
    if_cond: Option<Vec<Boolean<Context>>>,

    /// The required classifications for this step.
    classifications: Option<HashSet<String>>,

    // The required options for this step.
    options: Option<HashSet<String>>,

    /// If true this is considered the "zero time" for this sequence.
    zero_time: Option<bool>,

    /// The total duration of this step.
    #[serde(with = "robotica_common::datetime::with_duration")]
    duration: Duration,

    /// The latest time this step can be completed.
    #[serde(with = "robotica_common::datetime::with_option_duration")]
    #[serde(default)]
    latest_time: Option<Duration>,

    /// How frequently this step should be repeated.
    #[serde(with = "robotica_common::datetime::with_option_duration")]
    #[serde(default)]
    repeat_time: Option<Duration>,

    /// How many times this step should be repeated.
    repeat_count: Option<u8>,

    /// The tasks to execute.
    tasks: Vec<ConfigTask>,
}

impl Config {
    fn repeat_count(&self) -> u8 {
        self.repeat_count.unwrap_or(1)
    }
    fn repeat_time(&self) -> Duration {
        self.repeat_time.unwrap_or_else(|| duration::minutes(1))
    }
}

/// The configuration for a sequence.
pub type ConfigMap = HashMap<String, Vec<Config>>;

/// The schedule with all values completed.
///
/// Note this is a copy of the struct from robotica-common, because
/// it has some fields that are not serializable.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct Sequence {
    /// The title of the sequence.
    pub title: String,

    /// The id of the sequence.
    pub id: String,

    /// The source schedule date of the sequence.
    pub schedule_date: NaiveDate,

    /// The importance of the sequence.
    pub importance: Importance,

    /// The name of this sequence.
    #[serde(skip)]
    pub sequence_name: String,

    /// The conditions that must be true before this is scheduled.
    #[serde(skip)]
    pub if_cond: Option<Vec<Boolean<Context>>>,

    /// The required classifications for this step.
    #[serde(skip)]
    pub classifications: Option<HashSet<String>>,

    /// The required options for this step.
    #[serde(skip)]
    pub options: Option<HashSet<String>>,

    /// If true this is considered the "zero time" for this sequence.
    #[serde(skip)]
    pub zero_time: bool,

    /// The start time of this step.
    pub start_time: DateTime<Utc>,

    /// The end time of this step.
    pub end_time: DateTime<Utc>,

    /// The required duration of this step.
    #[serde(with = "robotica_common::datetime::with_duration")]
    pub duration: Duration,

    /// The latest time this step can be started.
    pub latest_time: DateTime<Utc>,

    /// The number of the repeat, starting from 1.
    pub repeat_number: usize,

    /// The tasks to execute.
    pub tasks: Vec<Task>,

    /// The status for this task - for use by executor.
    pub status: Option<Status>,

    /// The mark for this task - for use by executor.
    pub mark: Option<Mark>,
}

impl Sequence {
    /// Returns true if the sequence is done.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.status.map_or(false, |status| status.is_done())
    }
}

/// The context for the sequencer operation.
#[derive(Debug, Clone)]
pub struct Context {
    today: HashSet<String>,
    tomorrow: HashSet<String>,
    options: HashSet<String>,
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
        .sets
        .insert("options".to_string(), field_ref_of!(Context => options));
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
    /// Error reading the file
    #[error("Error reading file {0}: {1}")]
    FileError(PathBuf, std::io::Error),

    /// Error reading the file
    #[error("Error parsing file {0}: {1}")]
    YamlError(PathBuf, serde_yaml::Error),
}

/// An error loading the Config
#[derive(Error, Debug)]
pub enum ConfigCheckError {
    /// Environment variable not set
    #[error("Sequence {0} could not be found")]
    SequenceError(String),
}

/// Load the scheduler config from the given path.
///
/// # Errors
///
/// If the file cannot be read or parsed.
pub fn load_config(filename: &Path) -> Result<ConfigMap, ConfigError> {
    let f = std::fs::File::open(filename)
        .map_err(|e| ConfigError::FileError(filename.to_path_buf(), e))?;

    let config: ConfigMap = serde_yaml::from_reader(f)
        .map_err(|e| ConfigError::YamlError(filename.to_path_buf(), e))?;

    Ok(config)
}

/// Load the classification config from the environment variable `SEQUENCES_FILE`.
///
/// # Errors
///
/// Returns an error if the environment variable `SEQUENCES_FILE` is not set or if the file cannot be read.
// pub fn load_config_from_default_file() -> Result<ConfigMap, ConfigError> {
//     let env_name = "SEQUENCES_FILE";
//     let filename = get_env_os(env_name)?;
//     load_config(Path::new(&filename))
// }

/// Check the config for errors.
///
/// # Errors
///
/// Returns an error if a sequence is referenced that does not exist.
pub fn check_schedule(
    schedule: &[scheduler::Config],
    sequence: &ConfigMap,
) -> Result<(), ConfigCheckError> {
    for s in schedule {
        s.get_sequences().keys().try_for_each(|k| {
            if sequence.contains_key(k) {
                Ok(())
            } else {
                Err(ConfigCheckError::SequenceError(k.to_string()))
            }
        })?;
    }
    Ok(())
}

/// Something went wrong trying to get a sequence.
#[derive(Error, Debug)]
pub enum SequenceError {
    /// The sequence is could not be found.
    #[error("No sequence found for {0}")]
    NoSequence(String),
}

const fn map_qos(qos: Option<u8>) -> QoS {
    match qos {
        Some(0) => QoS::AtMostOnce,
        Some(1) => QoS::AtLeastOnce,
        // Some(2) => QoS::ExactlyOnce,
        _ => QoS::ExactlyOnce,
    }
}

fn config_to_sequence(
    sequence_name: &str,
    config: Config,
    start_time: &DateTime<Utc>,
    id: String,
    schedule_date: NaiveDate,
    repeat_number: usize,
) -> Sequence {
    let tasks = config
        .tasks
        .into_iter()
        .map(|src_task| Task {
            title: src_task.title,
            payload: src_task
                .payload
                .unwrap_or_else(|| Payload::String(String::new())),
            qos: map_qos(src_task.qos),
            retain: src_task.retain.unwrap_or(Retain::NoRetain),
            topics: src_task.topics,
        })
        .collect();

    let default_latest_time = duration::minutes(1);
    let latest_time = *start_time + config.latest_time.unwrap_or(default_latest_time);

    #[allow(deprecated)]
    Sequence {
        title: config.title,
        id,
        schedule_date,
        importance: config.importance,
        sequence_name: sequence_name.to_string(),
        if_cond: config.if_cond,
        classifications: config.classifications,
        options: config.options,
        zero_time: config.zero_time.unwrap_or(false),
        start_time: *start_time,
        end_time: *start_time + config.duration,
        duration: config.duration,
        latest_time,
        repeat_number,
        tasks,
        mark: None,
        status: None,
    }
}

/// Get the sequence for the given classification.
///
/// # Errors
///
/// Returns an error if the sequence is not found.
#[allow(clippy::implicit_hasher)]
pub fn get_sequence_with_config(
    config: &ConfigMap,
    schedule_date: NaiveDate,
    sequence_name: &str,
    today: &HashSet<String>,
    tomorrow: &HashSet<String>,
    options: &HashSet<String>,
    start_time: &DateTime<Utc>,
) -> Result<Vec<Sequence>, SequenceError> {
    let start_time = *start_time;

    let src_sequences: &Vec<Config> = config
        .get(sequence_name)
        .ok_or_else(|| SequenceError::NoSequence(sequence_name.to_string()))?;

    let context = Context {
        today: today.clone(),
        tomorrow: tomorrow.clone(),
        options: options.clone(),
    };

    let expanded_list = src_sequences
        .iter()
        .enumerate()
        .filter(|(_n, config)| filter_sequence(config, &context))
        .flat_map(expand_config)
        .collect::<Vec<_>>();

    let mut start_time = get_corrected_start_time(start_time, &expanded_list);
    let mut sequences: Vec<Sequence> = Vec::with_capacity(expanded_list.len());

    for expanded in &expanded_list {
        let id = expanded.config.id.as_ref().map_or_else(
            || format!("{sequence_name}_{}", expanded.number),
            std::clone::Clone::clone,
        );
        let sequence = config_to_sequence(
            sequence_name,
            expanded.config.clone(),
            &start_time,
            id,
            schedule_date,
            expanded.repeat_number,
        );
        sequences.push(sequence);
        start_time += expanded.duration;
    }

    Ok(sequences)
}

fn filter_sequence(config: &Config, context: &Context) -> bool {
    let mut ok = true;
    if let Some(classifications) = &config.classifications {
        if context.today.intersection(classifications).next().is_none() {
            ok = false;
        }
    }
    if let Some(test_options) = &config.options {
        if context.options.intersection(test_options).next().is_none() {
            ok = false;
        }
    }
    if let Some(if_cond) = &config.if_cond {
        if !if_cond.iter().any(|c| c.eval(context)) {
            ok = false;
        }
    }
    ok
}

#[derive(Clone)]
struct ExpandedConfig<'a> {
    config: &'a Config,
    number: usize,
    repeat_number: usize,
    duration: Duration,
}

fn expand_config((n, config): (usize, &Config)) -> Vec<ExpandedConfig> {
    let repeat_count = config.repeat_count() as usize;
    let mut out = Vec::with_capacity(repeat_count);

    for i in 1..repeat_count {
        let duration = config.repeat_time();
        out.push(ExpandedConfig {
            config,
            number: n,
            repeat_number: i,
            duration,
        });
    }

    if repeat_count >= 1 {
        let duration = config.duration;
        out.push(ExpandedConfig {
            config,
            number: n,
            repeat_number: repeat_count,
            duration,
        });
    }

    out
}

/// Get the sequence for the given schedule.
///
/// # Errors
///
/// Returns an error if the sequence is not found.
#[allow(clippy::implicit_hasher)]
pub fn schedule_list_to_sequence(
    config_map: &ConfigMap,
    schedule_date: NaiveDate,
    schedule_list: &Vec<scheduler::Schedule>,
    today: &HashSet<String>,
    tomorrow: &HashSet<String>,
) -> Result<Vec<Sequence>, SequenceError> {
    let mut sequences = Vec::new();

    for schedule in schedule_list {
        let mut sequence = get_sequence_with_config(
            config_map,
            schedule_date,
            &schedule.sequence_name,
            today,
            tomorrow,
            &schedule.options,
            &schedule.datetime,
        )?;
        sequences.append(&mut sequence);
    }

    // Sort the sequences by the start, end time.
    sequences.sort_by_key(|s| (s.start_time, s.end_time));

    Ok(sequences)
}

fn get_corrected_start_time(
    start_time: DateTime<Utc>,
    expanded_list: &Vec<ExpandedConfig>,
) -> DateTime<Utc> {
    let mut updated_start_time = start_time;

    for expanded in expanded_list {
        if expanded.config.zero_time == Some(true) {
            return updated_start_time;
        }

        updated_start_time -= expanded.duration;
    }

    // If we didn't find any zero time sequences, then just return the start time.
    start_time
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use chrono::TimeZone;

    use crate::scheduling::scheduler;

    use super::*;

    #[test]
    fn test_load_test_file() {
        let config = load_config(Path::new("test/sequences.yaml")).unwrap();
        assert!(config.contains_key("open_presents"));
    }

    #[test]
    fn test_get_sequence() {
        let config = vec![
            Config {
                title: "test".to_string(),
                id: None,
                importance: Importance::Medium,
                classifications: Some(HashSet::from(["christmas".to_string()])),
                options: Some(HashSet::from(["boxing".to_string()])),
                if_cond: None,
                zero_time: Some(true),
                duration: duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    title: "task 1".to_string(),
                    payload: None,
                    qos: None,
                    retain: None,
                    topics: vec!["test/test".to_string()],
                }],
            },
            Config {
                title: "test".to_string(),
                id: None,
                importance: Importance::Medium,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(false),
                duration: duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    title: "task 2".to_string(),
                    payload: None,
                    qos: None,
                    retain: None,
                    topics: vec!["test/test".to_string()],
                }],
            },
        ];

        let config_map = ConfigMap::from([("test".to_string(), config)]);
        let sequence = get_sequence_with_config(
            &config_map,
            NaiveDate::from_ymd_opt(2020, 12, 25).unwrap(),
            "test",
            &HashSet::from(["christmas".to_string()]),
            &HashSet::new(),
            &HashSet::from(["boxing".to_string()]),
            &Utc.with_ymd_and_hms(2020, 12, 25, 0, 0, 0).unwrap(),
        )
        .unwrap();

        assert_eq!(sequence.len(), 2);
        assert_eq!(
            sequence[0].start_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 0, 0).unwrap()
        );
        assert_eq!(
            sequence[0].latest_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 1, 0).unwrap()
        );
        assert_eq!(sequence[0].id, "test_0");
        assert_eq!(sequence[0].tasks.len(), 1);
        assert_eq!(sequence[0].tasks[0].title, "task 1");

        assert_eq!(
            sequence[1].start_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 30, 0).unwrap()
        );
        assert_eq!(
            sequence[1].latest_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 31, 0).unwrap()
        );
        assert_eq!(sequence[1].id, "test_1");
        assert_eq!(sequence[1].tasks.len(), 1);
        assert_eq!(sequence[1].tasks[0].title, "task 2");
    }

    #[test]
    fn test_filter_sequence() {
        let config = vec![
            Config {
                title: "test".to_string(),
                id: None,
                importance: Importance::Medium,
                classifications: Some(HashSet::from(["christmas".to_string()])),
                options: Some(HashSet::from(["boxing".to_string()])),
                if_cond: None,
                zero_time: Some(false),
                duration: duration::minutes(15),
                latest_time: None,
                repeat_count: Some(2),
                repeat_time: None,
                tasks: vec![ConfigTask {
                    title: "task 1".to_string(),
                    payload: None,
                    qos: None,
                    retain: None,
                    topics: vec!["test/test".to_string()],
                }],
            },
            Config {
                title: "test".to_string(),
                id: None,
                importance: Importance::Medium,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(false),
                duration: duration::minutes(15),
                latest_time: None,
                repeat_count: Some(2),
                repeat_time: None,
                tasks: vec![ConfigTask {
                    title: "task 2".to_string(),
                    payload: None,
                    qos: None,
                    retain: None,
                    topics: vec!["test/test".to_string()],
                }],
            },
        ];

        let context = Context {
            today: HashSet::from(["christmas".to_string()]),
            tomorrow: HashSet::from([]),
            options: HashSet::from(["boxing".to_string()]),
        };
        let result = filter_sequence(&config[0], &context);
        assert!(result);
        let result = filter_sequence(&config[1], &context);
        assert!(result);

        let context = Context {
            today: HashSet::from([]),
            tomorrow: HashSet::from([]),
            options: HashSet::from(["boxing".to_string()]),
        };
        let result = filter_sequence(&config[0], &context);
        assert!(!result);
        let result = filter_sequence(&config[1], &context);
        assert!(result);

        let context = Context {
            today: HashSet::from(["christmas".to_string()]),
            tomorrow: HashSet::from([]),
            options: HashSet::from([]),
        };
        let result = filter_sequence(&config[0], &context);
        assert!(!result);
        let result = filter_sequence(&config[1], &context);
        assert!(result);
    }

    #[test]
    fn test_expand_0() {
        let config = vec![Config {
            title: "test".to_string(),
            id: None,
            importance: Importance::Medium,
            classifications: Some(HashSet::from(["christmas".to_string()])),
            options: Some(HashSet::from(["boxing".to_string()])),
            if_cond: None,
            zero_time: Some(false),
            duration: duration::minutes(15),
            latest_time: None,
            repeat_count: Some(0),
            repeat_time: Some(duration::minutes(5)),
            tasks: vec![ConfigTask {
                title: "task 1".to_string(),
                payload: None,
                qos: None,
                retain: None,
                topics: vec!["test/test".to_string()],
            }],
        }];

        let config: Vec<(usize, &Config)> = config.iter().enumerate().collect();

        let result = expand_config(config[0]);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_expand_1() {
        let config = vec![Config {
            title: "test".to_string(),
            id: None,
            importance: Importance::Medium,
            classifications: Some(HashSet::from(["christmas".to_string()])),
            options: Some(HashSet::from(["boxing".to_string()])),
            if_cond: None,
            zero_time: Some(false),
            duration: duration::minutes(15),
            latest_time: None,
            repeat_count: Some(1),
            repeat_time: Some(duration::minutes(5)),
            tasks: vec![ConfigTask {
                title: "task 1".to_string(),
                payload: None,
                qos: None,
                retain: None,
                topics: vec!["test/test".to_string()],
            }],
        }];

        let config: Vec<(usize, &Config)> = config.iter().enumerate().collect();

        let result = expand_config(config[0]);
        assert_eq!(result.len(), 1);

        assert_eq!(result[0].duration, duration::minutes(15));
        assert_eq!(result[0].number, 0);
        assert_eq!(result[0].repeat_number, 1);
    }

    #[test]
    fn test_expand_3() {
        let config = vec![
            Config {
                title: "test".to_string(),
                id: None,
                importance: Importance::Medium,
                classifications: Some(HashSet::from(["christmas".to_string()])),
                options: Some(HashSet::from(["boxing".to_string()])),
                if_cond: None,
                zero_time: Some(false),
                duration: duration::minutes(15),
                latest_time: None,
                repeat_count: Some(3),
                repeat_time: Some(duration::minutes(5)),
                tasks: vec![ConfigTask {
                    title: "task 1".to_string(),
                    payload: None,
                    qos: None,
                    retain: None,
                    topics: vec!["test/test".to_string()],
                }],
            },
            Config {
                title: "test".to_string(),
                id: None,
                importance: Importance::Medium,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(false),
                duration: duration::minutes(15),
                latest_time: None,
                repeat_count: Some(3),
                repeat_time: None,
                tasks: vec![ConfigTask {
                    title: "task 2".to_string(),
                    payload: None,
                    qos: None,
                    retain: None,
                    topics: vec!["test/test".to_string()],
                }],
            },
        ];

        let config: Vec<(usize, &Config)> = config.iter().enumerate().collect();

        let result = expand_config(config[0]);
        assert_eq!(result.len(), 3);

        assert_eq!(result[0].duration, duration::minutes(5));
        assert_eq!(result[0].number, 0);
        assert_eq!(result[0].repeat_number, 1);

        assert_eq!(result[1].duration, duration::minutes(5));
        assert_eq!(result[1].number, 0);
        assert_eq!(result[1].repeat_number, 2);

        assert_eq!(result[2].duration, duration::minutes(15));
        assert_eq!(result[2].number, 0);
        assert_eq!(result[2].repeat_number, 3);

        let result = expand_config(config[1]);
        assert_eq!(result.len(), 3);

        assert_eq!(result[0].duration, duration::minutes(1));
        assert_eq!(result[0].number, 1);
        assert_eq!(result[0].repeat_number, 1);

        assert_eq!(result[1].duration, duration::minutes(1));
        assert_eq!(result[1].number, 1);
        assert_eq!(result[1].repeat_number, 2);

        assert_eq!(result[2].duration, duration::minutes(15));
        assert_eq!(result[2].number, 1);
        assert_eq!(result[2].repeat_number, 3);
    }

    #[test]
    fn test_get_corrected_start_time_0() {
        let config = Config {
            title: "test".to_string(),
            id: None,
            importance: Importance::Medium,
            classifications: Some(HashSet::from(["christmas".to_string()])),
            options: Some(HashSet::from(["boxing".to_string()])),
            if_cond: None,
            zero_time: Some(false),
            duration: duration::minutes(15),
            latest_time: None,
            repeat_count: Some(2),
            repeat_time: None,
            tasks: vec![ConfigTask {
                title: "task 1".to_string(),
                payload: None,
                qos: None,
                retain: None,
                topics: vec!["test/test".to_string()],
            }],
        };

        let expanded = vec![
            ExpandedConfig {
                number: 0,
                repeat_number: 1,
                duration: duration::minutes(15),
                config: &config,
            },
            ExpandedConfig {
                number: 0,
                repeat_number: 1,
                duration: duration::minutes(15),
                config: &config,
            },
        ];

        let datetime = Utc.with_ymd_and_hms(2020, 12, 25, 0, 30, 0).unwrap();
        let corrected_datetime = get_corrected_start_time(datetime, &expanded);

        assert_eq!(
            corrected_datetime,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 30, 0).unwrap()
        );
    }

    #[test]
    fn test_get_corrected_start_time_1() {
        let config = Config {
            title: "test".to_string(),
            id: None,
            importance: Importance::Medium,
            classifications: Some(HashSet::from(["christmas".to_string()])),
            options: Some(HashSet::from(["boxing".to_string()])),
            if_cond: None,
            zero_time: Some(true),
            duration: duration::minutes(15),
            latest_time: None,
            repeat_count: Some(2),
            repeat_time: None,
            tasks: vec![ConfigTask {
                title: "task 1".to_string(),
                payload: None,
                qos: None,
                retain: None,
                topics: vec!["test/test".to_string()],
            }],
        };

        let expanded = vec![
            ExpandedConfig {
                number: 0,
                repeat_number: 1,
                duration: duration::minutes(15),
                config: &config,
            },
            ExpandedConfig {
                number: 0,
                repeat_number: 1,
                duration: duration::minutes(15),
                config: &config,
            },
        ];

        let datetime = Utc.with_ymd_and_hms(2020, 12, 25, 0, 30, 0).unwrap();
        let corrected_datetime = get_corrected_start_time(datetime, &expanded);

        assert_eq!(
            corrected_datetime,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 30, 0).unwrap()
        );
    }

    #[test]
    fn test_get_corrected_start_time_2() {
        let config = Config {
            title: "test".to_string(),
            id: None,
            importance: Importance::Medium,
            classifications: Some(HashSet::from(["christmas".to_string()])),
            options: Some(HashSet::from(["boxing".to_string()])),
            if_cond: None,
            zero_time: Some(false),
            duration: duration::minutes(15),
            latest_time: None,
            repeat_count: Some(2),
            repeat_time: None,
            tasks: vec![ConfigTask {
                title: "task 1".to_string(),
                payload: None,
                qos: None,
                retain: None,
                topics: vec!["test/test".to_string()],
            }],
        };

        let config2 = Config {
            zero_time: Some(true),
            ..config.clone()
        };

        let expanded = vec![
            ExpandedConfig {
                number: 0,
                repeat_number: 1,
                duration: duration::minutes(15),
                config: &config,
            },
            ExpandedConfig {
                number: 0,
                repeat_number: 1,
                duration: duration::minutes(15),
                config: &config2,
            },
        ];

        let datetime = Utc.with_ymd_and_hms(2020, 12, 25, 0, 30, 0).unwrap();
        let corrected_datetime = get_corrected_start_time(datetime, &expanded);

        assert_eq!(
            corrected_datetime,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 15, 0).unwrap()
        );
    }

    #[test]
    fn test_get_sequence_zero_time() {
        let config = vec![
            Config {
                title: "test".to_string(),
                id: None,
                importance: Importance::Medium,
                classifications: Some(HashSet::from(["christmas".to_string()])),
                options: Some(HashSet::from(["boxing".to_string()])),
                if_cond: None,
                zero_time: Some(false),
                duration: duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    title: "task 1".to_string(),
                    payload: None,
                    qos: None,
                    retain: None,
                    topics: vec!["test/test".to_string()],
                }],
            },
            Config {
                title: "test".to_string(),
                id: None,
                importance: Importance::Medium,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(true),
                duration: duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    title: "task 2".to_string(),
                    payload: None,
                    qos: None,
                    retain: None,
                    topics: vec!["test/test".to_string()],
                }],
            },
        ];

        let config_map = ConfigMap::from([("test".to_string(), config)]);
        let sequence = get_sequence_with_config(
            &config_map,
            NaiveDate::from_ymd_opt(2020, 12, 25).unwrap(),
            "test",
            &HashSet::from(["christmas".to_string()]),
            &HashSet::new(),
            &HashSet::from(["boxing".to_string()]),
            &Utc.with_ymd_and_hms(2020, 12, 25, 0, 30, 0).unwrap(),
        )
        .unwrap();

        assert_eq!(sequence.len(), 2);
        assert_eq!(
            sequence[0].start_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 0, 0).unwrap()
        );
        assert_eq!(
            sequence[0].latest_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 1, 0).unwrap()
        );
        assert_eq!(sequence[0].id, "test_0");
        assert_eq!(sequence[0].tasks.len(), 1);

        assert_eq!(
            sequence[1].start_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 30, 0).unwrap()
        );
        assert_eq!(
            sequence[1].latest_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 31, 0).unwrap()
        );
        assert_eq!(sequence[1].id, "test_1");
        assert_eq!(sequence[1].tasks.len(), 1);
    }

    #[test]
    fn test_get_sequence_repeat() {
        let config = vec![Config {
            title: "test".to_string(),
            id: None,
            importance: Importance::Medium,
            classifications: Some(HashSet::from(["christmas".to_string()])),
            options: Some(HashSet::from(["boxing".to_string()])),
            if_cond: None,
            zero_time: Some(true),
            duration: duration::minutes(30),
            latest_time: None,
            repeat_count: Some(2),
            repeat_time: Some(duration::minutes(10)),
            tasks: vec![ConfigTask {
                title: "task 1".to_string(),
                payload: None,
                qos: None,
                retain: None,
                topics: vec!["test/test".to_string()],
            }],
        }];

        let config_map = ConfigMap::from([("test".to_string(), config)]);
        let sequence = get_sequence_with_config(
            &config_map,
            NaiveDate::from_ymd_opt(2020, 12, 25).unwrap(),
            "test",
            &HashSet::from(["christmas".to_string()]),
            &HashSet::new(),
            &HashSet::from(["boxing".to_string()]),
            &Utc.with_ymd_and_hms(2020, 12, 25, 0, 0, 0).unwrap(),
        )
        .unwrap();

        assert_eq!(sequence.len(), 2);
        assert_eq!(
            sequence[0].start_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 0, 0).unwrap()
        );
        assert_eq!(
            sequence[0].latest_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 1, 0).unwrap()
        );
        assert_eq!(sequence[0].id, "test_0");
        assert_eq!(sequence[0].tasks.len(), 1);

        assert_eq!(
            sequence[1].start_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 10, 0).unwrap()
        );
        assert_eq!(
            sequence[1].latest_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 11, 0).unwrap()
        );
        assert_eq!(sequence[1].id, "test_0");
        assert_eq!(sequence[1].tasks.len(), 1);
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn test_schedule_to_sequence() {
        let schedule = vec![
            scheduler::Schedule {
                sequence_name: "test".to_string(),
                options: HashSet::new(),
                datetime: Utc.with_ymd_and_hms(2020, 12, 25, 0, 0, 0).unwrap(),
            },
            scheduler::Schedule {
                sequence_name: "christmas".to_string(),
                options: HashSet::new(),
                datetime: Utc.with_ymd_and_hms(2020, 12, 25, 0, 10, 0).unwrap(),
            },
        ];

        let config_test = vec![
            Config {
                title: "test".to_string(),
                id: None,
                importance: Importance::Medium,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(true),
                duration: duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    title: "task 1".to_string(),
                    payload: None,
                    qos: None,
                    retain: None,
                    topics: vec!["test/test".to_string()],
                }],
            },
            Config {
                title: "test".to_string(),
                id: None,
                importance: Importance::Medium,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(false),
                duration: duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    title: "task 2".to_string(),
                    payload: None,
                    qos: None,
                    retain: None,
                    topics: vec!["test/test".to_string()],
                }],
            },
        ];

        let config_christmas = vec![
            Config {
                title: "test".to_string(),
                id: None,
                importance: Importance::Medium,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(true),
                duration: duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    title: "task 3".to_string(),
                    payload: None,
                    qos: None,
                    retain: None,
                    topics: vec!["test/test".to_string()],
                }],
            },
            Config {
                title: "test".to_string(),
                id: None,
                importance: Importance::Medium,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(false),
                duration: duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    title: "task 4".to_string(),
                    payload: None,
                    qos: None,
                    retain: None,
                    topics: vec!["test/test".to_string()],
                }],
            },
        ];

        let config_map = ConfigMap::from([
            ("test".to_string(), config_test),
            ("christmas".to_string(), config_christmas),
        ]);
        let sequence = schedule_list_to_sequence(
            &config_map,
            NaiveDate::from_ymd_opt(2020, 12, 25).unwrap(),
            &schedule,
            &HashSet::new(),
            &HashSet::new(),
        )
        .unwrap();

        assert_eq!(sequence.len(), 4);
        assert_eq!(
            sequence[0].start_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 0, 0).unwrap()
        );
        assert_eq!(
            sequence[0].latest_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 1, 0).unwrap()
        );
        assert_eq!(sequence[0].id, "test_0");
        assert_eq!(sequence[0].tasks.len(), 1);
        assert_eq!(sequence[0].tasks[0].title, "task 1");

        assert_eq!(
            sequence[1].start_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 10, 0).unwrap()
        );
        assert_eq!(
            sequence[1].latest_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 11, 0).unwrap()
        );
        assert_eq!(sequence[1].id, "christmas_0");
        assert_eq!(sequence[1].tasks.len(), 1);
        assert_eq!(sequence[1].tasks[0].title, "task 3");

        assert_eq!(
            sequence[2].start_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 30, 0).unwrap()
        );
        assert_eq!(
            sequence[2].latest_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 31, 0).unwrap()
        );
        assert_eq!(sequence[2].id, "test_1");
        assert_eq!(sequence[2].tasks.len(), 1);
        assert_eq!(sequence[2].tasks[0].title, "task 2");

        assert_eq!(
            sequence[3].start_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 40, 0).unwrap()
        );
        assert_eq!(
            sequence[3].latest_time,
            Utc.with_ymd_and_hms(2020, 12, 25, 0, 41, 0).unwrap()
        );
        assert_eq!(sequence[3].id, "christmas_1");
        assert_eq!(sequence[3].tasks.len(), 1);
        assert_eq!(sequence[3].tasks[0].title, "task 4");
    }

    #[test]
    fn test_check_schedule_bad() {
        let schedule: Vec<scheduler::Config> = scheduler::create_test_config();

        let sequence = HashMap::new();
        let err = check_schedule(&schedule, &sequence);
        assert!(err.is_err());
        assert!(matches!(err, Err(ConfigCheckError::SequenceError(_))));
    }

    #[test]
    fn test_check_schedule_good() {
        let schedule: Vec<scheduler::Config> = scheduler::create_test_config();

        let sequence = HashMap::from([
            (
                "wake_up".to_string(),
                vec![Config {
                    title: "test".to_string(),
                    id: None,
                    importance: Importance::Medium,
                    classifications: None,
                    options: None,
                    if_cond: None,
                    zero_time: Some(true),
                    duration: duration::minutes(30),
                    latest_time: None,
                    repeat_count: None,
                    repeat_time: None,
                    tasks: vec![ConfigTask {
                        title: "task".to_string(),
                        payload: None,
                        qos: None,
                        retain: None,
                        topics: vec!["test/test".to_string()],
                    }],
                }],
            ),
            (
                "sleep".to_string(),
                vec![Config {
                    title: "test".to_string(),
                    id: None,
                    importance: Importance::Medium,
                    classifications: None,
                    options: None,
                    if_cond: None,
                    zero_time: Some(false),
                    duration: duration::minutes(30),
                    latest_time: None,
                    repeat_count: None,
                    repeat_time: None,
                    tasks: vec![ConfigTask {
                        title: "task 1".to_string(),
                        payload: None,
                        qos: None,
                        retain: None,
                        topics: vec!["test/test".to_string()],
                    }],
                }],
            ),
        ]);
        check_schedule(&schedule, &sequence).unwrap();
    }
}
