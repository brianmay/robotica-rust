//! Assemble schedule into sequences
use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
};

use chrono::Utc;
use field_ref::field_ref_of;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::sources::mqtt;

use super::{
    ast::{Boolean, Fields},
    conditions, scheduler,
    types::{DateTime, Duration, Mark},
};

/// Payload in a task.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum Payload {
    /// A string payload.
    #[serde(rename = "payload_str")]
    String(String),

    /// A JSON payload.
    #[serde(rename = "payload_json")]
    Json(Value),
}

/// A task in a config sequence.
#[derive(Deserialize, Debug, Clone)]
pub struct ConfigTask {
    /// The description of the task.
    pub description: Option<String>,

    /// The payload of the task.
    #[serde(flatten)]
    pub payload: Option<Payload>,

    /// The qos to be used when sending the message.
    qos: Option<u8>,

    /// The retain flag to be used when sending the message.
    retain: Option<bool>,

    /// The locations this task acts on.
    locations: Vec<String>,

    /// The devices this task acts on.
    devices: Vec<String>,

    /// The topics this task will send to.
    ///
    /// If this is not specified, the topic will be generated from the locations and devices.
    topics: Option<Vec<String>>,
}

/// The source schedule loaded from the config file.
#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    /// The id of the step.
    pub id: Option<String>,

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
    required_time: Duration,

    /// The latest time this step can be completed.
    latest_time: Option<Duration>,

    /// How frequently this step should be repeated.
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
}

/// The configuration for a sequence.
pub type ConfigMap = HashMap<String, Vec<Config>>;

/// A task with all values completed.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct Task {
    /// The description of the task.
    pub description: Option<String>,

    /// The payload of the task.
    #[serde(flatten)]
    pub payload: Payload,

    /// The qos to be used when sending the message.
    qos: mqtt::QoS,

    /// The retain value to be used when sending the message.
    #[serde(skip)]
    retain: bool,

    /// The locations this task acts on.
    locations: Vec<String>,

    /// The devices this task acts on.
    devices: Vec<String>,

    /// The topics this task will send to.
    ///
    /// If this is not specified, the topic will be generated from the locations and devices.
    topics: Vec<String>,
}

impl Task {
    /// Get the MQTT message for this task.
    #[must_use]
    pub fn get_messages(&self) -> Vec<mqtt::Message> {
        let mut messages = Vec::with_capacity(self.topics.len());
        for topic in &self.topics {
            let payload = match &self.payload {
                Payload::String(s) => s.to_string(),
                Payload::Json(v) => v.to_string(),
            };
            let message = mqtt::Message::from_string(topic, &payload, self.retain, self.qos);
            messages.push(message);
        }
        messages
    }
}

/// The schedule with all values completed.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct Sequence {
    /// The id of the step.
    pub id: String,

    /// The name of this sequence.
    #[serde(skip)]
    pub sequence_name: String,

    /// The conditions that must be true before this is scheduled.
    #[serde(skip)]
    if_cond: Option<Vec<Boolean<Context>>>,

    /// The required classifications for this step.
    #[serde(skip)]
    classifications: Option<HashSet<String>>,

    // The required options for this step.
    #[serde(skip)]
    options: Option<HashSet<String>>,

    /// If true this is considered the "zero time" for this sequence.
    #[serde(skip)]
    zero_time: bool,

    /// The start time of this step.
    pub required_time: DateTime<Utc>,

    /// The required duration of this step.
    #[serde(skip)]
    required_duration: Duration,

    /// The latest time this step can be completed.
    pub latest_time: DateTime<Utc>,

    /// The number of the repeat.
    repeat_number: u8,

    /// The tasks to execute.
    pub tasks: Vec<Task>,

    /// The mark for this task - for use by executor.
    pub mark: Option<Mark>,
}

/// A Map of Sequences.
pub type SequenceMap = HashMap<String, Vec<Sequence>>;

impl Ord for Sequence {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.required_time.cmp(&other.required_time)
    }
}

impl PartialOrd for Sequence {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.required_time.partial_cmp(&other.required_time)
    }
}
impl Eq for Sequence {}

impl PartialEq for Sequence {
    fn eq(&self, other: &Self) -> bool {
        self.required_time == other.required_time
    }
}

#[derive(Debug, Clone)]
struct Context {
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
///
/// # Panics
///
/// Never (FIXME: check this).
pub fn load_config(filename: &Path) -> Result<ConfigMap, ConfigError> {
    let f = std::fs::File::open(&filename)
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
pub fn load_config_from_default_file() -> Result<ConfigMap, ConfigError> {
    let env_name = "SEQUENCES_FILE";
    let filename = env::var(env_name).map_err(|_| ConfigError::VarError(env_name.to_string()))?;
    load_config(Path::new(&filename))
}

/// Something went wrong trying to get a sequence.
#[derive(Error, Debug)]
pub enum SequenceError {
    /// The sequence is could not be found.
    #[error("No sequence found for {0}")]
    NoSequence(String),
}

const fn map_qos(qos: Option<u8>) -> mqtt::QoS {
    match qos {
        Some(0) => mqtt::QoS::at_most_once(),
        Some(1) => mqtt::QoS::at_least_once(),
        // Some(2) => mqtt::QoS::ExactlyOnce(),
        _ => mqtt::QoS::exactly_once(),
    }
}

fn config_to_sequence(
    sequence_name: &str,
    config: Config,
    start_time: &DateTime<Utc>,
    id: String,
    repeat_number: u8,
) -> Sequence {
    let mut tasks = Vec::with_capacity(config.tasks.len());
    config.tasks.into_iter().for_each(|src_task| {
        let topics = if let Some(topics) = src_task.topics {
            topics
        } else {
            let mut topics = Vec::with_capacity(src_task.locations.len() * src_task.devices.len());
            src_task.locations.iter().for_each(|l| {
                src_task
                    .devices
                    .iter()
                    .for_each(|d| topics.push(format!("command/{}/{}", l, d)));
            });
            topics
        };

        let task = Task {
            description: src_task.description,
            payload: src_task
                .payload
                .unwrap_or_else(|| Payload::String("".to_string())),
            qos: map_qos(src_task.qos),
            retain: src_task.retain.unwrap_or(false),
            locations: src_task.locations,
            devices: src_task.devices,
            topics,
        };

        tasks.push(task);
    });

    let default_latest_time = Duration::minutes(5);
    let latest_time = start_time.clone() + config.latest_time.unwrap_or(default_latest_time);

    Sequence {
        id,
        sequence_name: sequence_name.to_string(),
        if_cond: config.if_cond,
        classifications: config.classifications,
        options: config.options,
        zero_time: config.zero_time.unwrap_or(false),
        required_time: start_time.clone(),
        required_duration: config.required_time,
        latest_time,
        repeat_number,
        tasks,
        mark: None,
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
    sequence_name: &str,
    today: &HashSet<String>,
    tomorrow: &HashSet<String>,
    options: &HashSet<String>,
    start_time: &DateTime<Utc>,
) -> Result<Vec<Sequence>, SequenceError> {
    let start_time = start_time.clone();
    let default_repeat_time = Duration::seconds(0);

    let src_sequences = config
        .get(sequence_name)
        .ok_or_else(|| SequenceError::NoSequence(sequence_name.to_string()))?;

    let context = Context {
        today: today.clone(),
        tomorrow: tomorrow.clone(),
        options: options.clone(),
    };

    let mut start_time = get_corrected_start_time(start_time, src_sequences);
    let mut sequences = Vec::with_capacity(src_sequences.len());

    for (sequence_number, config) in src_sequences.iter().enumerate() {
        let mut ok = true;

        if let Some(classifications) = &config.classifications {
            if today.intersection(classifications).next().is_none() {
                ok = false;
            }
        }

        if let Some(test_options) = &config.options {
            if options.intersection(test_options).next().is_none() {
                ok = false;
            }
        }

        if let Some(if_cond) = &config.if_cond {
            if !if_cond.iter().any(|c| c.eval(&context)) {
                ok = false;
            }
        }

        if ok {
            let mut repeat_time = start_time.clone();
            let repeat_count = config.repeat_count();
            let id = match &config.id {
                Some(id) => id.clone(),
                None => format!("{}_{}", sequence_name, sequence_number),
            };
            for repeat in 0..repeat_count {
                let sequence = config_to_sequence(
                    sequence_name,
                    config.clone(),
                    &repeat_time,
                    id.clone(),
                    repeat,
                );
                repeat_time = repeat_time + config.repeat_time.unwrap_or(default_repeat_time);
                sequences.push(sequence);
            }
            start_time = start_time + config.required_time;
        }
    }

    Ok(sequences)
}

/// Get the sequence for the given schedule.
///
/// # Errors
///
/// Returns an error if the sequence is not found.
#[allow(clippy::implicit_hasher)]
pub fn schedule_list_to_sequence(
    config_map: &ConfigMap,
    schedule_list: &Vec<scheduler::Schedule>,
    today: &HashSet<String>,
    tomorrow: &HashSet<String>,
) -> Result<Vec<Sequence>, SequenceError> {
    let mut sequences = Vec::new();

    for schedule in schedule_list {
        let mut sequence = get_sequence_with_config(
            config_map,
            &schedule.sequence_name,
            today,
            tomorrow,
            &schedule.options,
            &schedule.datetime,
        )?;
        sequences.append(&mut sequence);
    }

    // Sort the sequences by the required time.
    sequences.sort();

    Ok(sequences)
}

fn get_corrected_start_time(start_time: DateTime<Utc>, config: &Vec<Config>) -> DateTime<Utc> {
    let mut updated_start_time = start_time.clone();

    for sequence in config {
        if sequence.zero_time == Some(true) {
            return updated_start_time;
        }

        // Note: original robotica code had a bug here, it was not taking into account repeats.
        for _ in 0..sequence.repeat_count() {
            updated_start_time = updated_start_time - sequence.required_time;
        }
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
                id: None,
                classifications: Some(HashSet::from(["christmas".to_string()])),
                options: Some(HashSet::from(["boxing".to_string()])),
                if_cond: None,
                zero_time: Some(true),
                required_time: Duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
            Config {
                id: None,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(false),
                required_time: Duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
        ];

        let config_map = ConfigMap::from([("test".to_string(), config)]);
        let sequence = get_sequence_with_config(
            &config_map,
            "test",
            &HashSet::from(["christmas".to_string()]),
            &HashSet::new(),
            &HashSet::from(["boxing".to_string()]),
            &Utc.ymd(2020, 12, 25).and_hms(0, 0, 0).into(),
        )
        .unwrap();

        assert_eq!(sequence.len(), 2);
        assert_eq!(
            sequence[0].required_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 0, 0).into()
        );
        assert_eq!(
            sequence[0].latest_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 5, 0).into()
        );
        assert_eq!(sequence[0].id, "test_0");
        assert_eq!(sequence[0].tasks.len(), 1);

        assert_eq!(
            sequence[1].required_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 30, 0).into()
        );
        assert_eq!(
            sequence[1].latest_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 35, 0).into()
        );
        assert_eq!(sequence[1].id, "test_1");
        assert_eq!(sequence[1].tasks.len(), 1);
    }

    #[test]
    fn test_get_corrected_start_time_0() {
        let config = vec![
            Config {
                id: None,
                classifications: Some(HashSet::from(["christmas".to_string()])),
                options: Some(HashSet::from(["boxing".to_string()])),
                if_cond: None,
                zero_time: Some(false),
                required_time: Duration::minutes(15),
                latest_time: None,
                repeat_count: Some(2),
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
            Config {
                id: None,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(false),
                required_time: Duration::minutes(15),
                latest_time: None,
                repeat_count: Some(2),
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
        ];

        let datetime = Utc.ymd(2020, 12, 25).and_hms(0, 30, 0).into();
        let corrected_datetime = get_corrected_start_time(datetime, &config);

        assert_eq!(
            corrected_datetime,
            Utc.ymd(2020, 12, 25).and_hms(0, 30, 0).into()
        );
    }

    #[test]
    fn test_get_corrected_start_time_1() {
        let config = vec![
            Config {
                id: None,
                classifications: Some(HashSet::from(["christmas".to_string()])),
                options: Some(HashSet::from(["boxing".to_string()])),
                if_cond: None,
                zero_time: Some(true),
                required_time: Duration::minutes(15),
                latest_time: None,
                repeat_count: Some(2),
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
            Config {
                id: None,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(false),
                required_time: Duration::minutes(15),
                latest_time: None,
                repeat_count: Some(2),
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
        ];

        let datetime = Utc.ymd(2020, 12, 25).and_hms(0, 30, 0).into();
        let corrected_datetime = get_corrected_start_time(datetime, &config);

        assert_eq!(
            corrected_datetime,
            Utc.ymd(2020, 12, 25).and_hms(0, 30, 0).into()
        );
    }

    #[test]
    fn test_get_corrected_start_time_2() {
        let config = vec![
            Config {
                id: None,
                classifications: Some(HashSet::from(["christmas".to_string()])),
                options: Some(HashSet::from(["boxing".to_string()])),
                if_cond: None,
                zero_time: Some(false),
                required_time: Duration::minutes(15),
                latest_time: None,
                repeat_count: Some(2),
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
            Config {
                id: None,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(true),
                required_time: Duration::minutes(15),
                latest_time: None,
                repeat_count: Some(2),
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
        ];
        let datetime = Utc.ymd(2020, 12, 25).and_hms(0, 30, 0).into();
        let corrected_datetime = get_corrected_start_time(datetime, &config);

        assert_eq!(
            corrected_datetime,
            Utc.ymd(2020, 12, 25).and_hms(0, 0, 0).into()
        );
    }

    #[test]
    fn test_get_sequence_zero_time() {
        let config = vec![
            Config {
                id: None,
                classifications: Some(HashSet::from(["christmas".to_string()])),
                options: Some(HashSet::from(["boxing".to_string()])),
                if_cond: None,
                zero_time: Some(false),
                required_time: Duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
            Config {
                id: None,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(true),
                required_time: Duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
        ];

        let config_map = ConfigMap::from([("test".to_string(), config)]);
        let sequence = get_sequence_with_config(
            &config_map,
            "test",
            &HashSet::from(["christmas".to_string()]),
            &HashSet::new(),
            &HashSet::from(["boxing".to_string()]),
            &Utc.ymd(2020, 12, 25).and_hms(0, 30, 0).into(),
        )
        .unwrap();

        assert_eq!(sequence.len(), 2);
        assert_eq!(
            sequence[0].required_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 0, 0).into()
        );
        assert_eq!(
            sequence[0].latest_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 5, 0).into()
        );
        assert_eq!(sequence[0].id, "test_0");
        assert_eq!(sequence[0].tasks.len(), 1);

        assert_eq!(
            sequence[1].required_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 30, 0).into()
        );
        assert_eq!(
            sequence[1].latest_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 35, 0).into()
        );
        assert_eq!(sequence[1].id, "test_1");
        assert_eq!(sequence[1].tasks.len(), 1);
    }

    #[test]
    fn test_get_sequence_repeat() {
        let config = vec![Config {
            id: None,
            classifications: Some(HashSet::from(["christmas".to_string()])),
            options: Some(HashSet::from(["boxing".to_string()])),
            if_cond: None,
            zero_time: Some(true),
            required_time: Duration::minutes(30),
            latest_time: None,
            repeat_count: Some(2),
            repeat_time: Some(Duration::minutes(10)),
            tasks: vec![ConfigTask {
                description: None,
                payload: None,
                qos: None,
                retain: None,
                locations: vec!["test".to_string()],
                devices: vec!["test".to_string()],
                topics: None,
            }],
        }];

        let config_map = ConfigMap::from([("test".to_string(), config)]);
        let sequence = get_sequence_with_config(
            &config_map,
            "test",
            &HashSet::from(["christmas".to_string()]),
            &HashSet::new(),
            &HashSet::from(["boxing".to_string()]),
            &Utc.ymd(2020, 12, 25).and_hms(0, 0, 0).into(),
        )
        .unwrap();

        assert_eq!(sequence.len(), 2);
        assert_eq!(
            sequence[0].required_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 0, 0).into()
        );
        assert_eq!(
            sequence[0].latest_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 5, 0).into()
        );
        assert_eq!(sequence[0].id, "test_0");
        assert_eq!(sequence[0].tasks.len(), 1);

        assert_eq!(
            sequence[1].required_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 10, 0).into()
        );
        assert_eq!(
            sequence[1].latest_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 15, 0).into()
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
                datetime: Utc.ymd(2020, 12, 25).and_hms(0, 0, 0).into(),
            },
            scheduler::Schedule {
                sequence_name: "christmas".to_string(),
                options: HashSet::new(),
                datetime: Utc.ymd(2020, 12, 25).and_hms(0, 10, 0).into(),
            },
        ];

        let config_test = vec![
            Config {
                id: None,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(true),
                required_time: Duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
            Config {
                id: None,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(false),
                required_time: Duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
        ];

        let config_christmas = vec![
            Config {
                id: None,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(true),
                required_time: Duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
            Config {
                id: None,
                classifications: None,
                options: None,
                if_cond: None,
                zero_time: Some(false),
                required_time: Duration::minutes(30),
                latest_time: None,
                repeat_count: None,
                repeat_time: None,
                tasks: vec![ConfigTask {
                    description: None,
                    payload: None,
                    qos: None,
                    retain: None,
                    locations: vec!["test".to_string()],
                    devices: vec!["test".to_string()],
                    topics: None,
                }],
            },
        ];

        let config_map = ConfigMap::from([
            ("test".to_string(), config_test),
            ("christmas".to_string(), config_christmas),
        ]);
        let sequence =
            schedule_list_to_sequence(&config_map, &schedule, &HashSet::new(), &HashSet::new())
                .unwrap();

        assert_eq!(sequence.len(), 4);
        assert_eq!(
            sequence[0].required_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 0, 0).into()
        );
        assert_eq!(
            sequence[0].latest_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 5, 0).into()
        );
        assert_eq!(sequence[0].id, "test_0");
        assert_eq!(sequence[0].tasks.len(), 1);

        assert_eq!(
            sequence[1].required_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 10, 0).into()
        );
        assert_eq!(
            sequence[1].latest_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 15, 0).into()
        );
        assert_eq!(sequence[1].id, "christmas_0");
        assert_eq!(sequence[1].tasks.len(), 1);

        assert_eq!(
            sequence[2].required_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 30, 0).into()
        );
        assert_eq!(
            sequence[2].latest_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 35, 0).into()
        );
        assert_eq!(sequence[2].id, "test_1");
        assert_eq!(sequence[2].tasks.len(), 1);

        assert_eq!(
            sequence[3].required_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 40, 0).into()
        );
        assert_eq!(
            sequence[3].latest_time,
            Utc.ymd(2020, 12, 25).and_hms(0, 45, 0).into()
        );
        assert_eq!(sequence[3].id, "christmas_1");
        assert_eq!(sequence[3].tasks.len(), 1);
    }
}
