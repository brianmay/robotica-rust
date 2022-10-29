//! Scheduler struct shared between robotica-rust and robotica-frontend
use std::{
    collections::HashSet,
    fmt::{Display, Formatter},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
// use thiserror::Error;

use crate::{
    datetime::{DateTime, Duration},
    mqtt,
};

/// The status of the Mark.
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub enum MarkStatus {
    /// The tasks are to be cancelled.
    #[serde(rename = "cancelled")]
    Cancelled,

    /// The tasks are already done.
    #[serde(rename = "done")]
    Done,
}

/// A mark on a step.
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct Mark {
    /// The id of the step.
    pub id: String,

    /// The status of the Mark.
    pub status: MarkStatus,

    /// The start time of the Mark.
    pub start_time: DateTime<Utc>,

    /// The end time of the Mark.
    pub stop_time: DateTime<Utc>,
}

// /// An error that can occur when parsing a mark.
// #[derive(Error, Debug)]
// pub enum MarkError {
//     /// The Mark is invalid.
//     #[error("Invalid mark {0}")]
//     ParseError(#[from] serde_json::Error),

//     /// UTF-8 error in Mark.
//     #[error("Invalid UTF8")]
//     Utf8Error(#[from] std::str::Utf8Error),
// }

// impl TryFrom<Message> for Mark {
//     type Error = MarkError;

//     fn try_from(msg: Message) -> Result<Self, Self::Error> {
//         let payload: String = msg.payload_into_string()?;
//         let mark: Mark = serde_json::from_str(&payload)?;
//         Ok(mark)
//     }
// }

/// Payload in a task.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Payload {
    /// A string payload.
    #[serde(rename = "payload_str")]
    String(String),

    /// A JSON payload.
    #[serde(rename = "payload_json")]
    Json(Value),
}

/// A task with all values completed.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Task {
    /// The description of the task.
    pub description: Option<String>,

    /// The payload of the task.
    #[serde(flatten)]
    pub payload: Payload,

    /// The qos to be used when sending the message.
    pub qos: mqtt::QoS,

    /// The retain value to be used when sending the message.
    pub retain: bool,

    /// The locations this task acts on.
    pub locations: Vec<String>,

    /// The devices this task acts on.
    pub devices: Vec<String>,

    /// The topics this task will send to.
    ///
    /// If this is not specified, the topic will be generated from the locations and devices.
    pub topics: Vec<String>,
}

fn location_device_to_text(location: &str, device: &str) -> String {
    format!("{}/{}", location, device)
}

fn command_to_text(command: &Value) -> String {
    let mtype = command
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    match mtype {
        "audio" => audio_command_to_text(command),
        "light" => light_command_to_text(command),
        "device" => device_command_to_text(command),
        "hdmi" => hdmi_command_to_text(command),
        _ => command.to_string(),
    }
}

fn audio_command_to_text(command: &Value) -> String {
    let message = command.get("message").and_then(Value::as_str);
    let music = command.get("music");
    let play_list = music
        .and_then(|music| music.get("play_list"))
        .and_then(Value::as_str);
    let music_stop = music
        .and_then(|music| music.get("stop"))
        .and_then(Value::as_bool);
    let volume = command.get("volume");
    let music_volume = volume
        .and_then(|volume| volume.get("music"))
        .and_then(Value::as_u64);
    let message_volume = volume
        .and_then(|volume| volume.get("message"))
        .and_then(Value::as_u64);
    let mut results = vec![];
    if let Some(volume) = message_volume {
        results.push(format!("set message volume {}", volume));
    }
    if let Some(message) = message {
        results.push(format!("say {}", message));
    }
    if let Some(music_stop) = music_stop {
        if music_stop {
            results.push("stop music".to_string());
        }
    }
    if let Some(volume) = music_volume {
        results.push(format!("set music volume {}", volume));
    }
    if let Some(play_list) = play_list {
        results.push(format!("play {}", play_list));
    }
    if results.is_empty() {
        "unknown audio command".to_string()
    } else {
        results.join(", ")
    }
}

fn light_command_to_text(command: &Value) -> String {
    let action = command.get("lights").and_then(Value::as_str);
    let scene = command.get("scene").and_then(Value::as_str);

    match (action, scene) {
        (Some(command), Some(scene)) => format!("{command} scene {scene}"),
        (Some(command), None) => command.to_string(),
        (None, Some(scene)) => format!("scene {scene}"),
        _ => "unknown light command".to_string(),
    }
}

fn device_command_to_text(command: &Value) -> String {
    let action = command.get("lights").and_then(Value::as_str);

    action.map_or_else(
        || "unknown device command".to_string(),
        |action| format!("Device #{action}"),
    )
}

fn hdmi_command_to_text(command: &Value) -> String {
    let source = command.get("source").and_then(Value::as_str);

    source.map_or_else(
        || "unknown hdmi command".to_string(),
        |source| format!("HDMI source #{source}"),
    )
}

impl Display for Task {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut location_list = vec![];

        for location in &self.locations {
            for device in &self.devices {
                location_list.push(location_device_to_text(location, device));
            }
        }

        if location_list.is_empty() {
            write!(f, "Nowhere: ")?;
        } else {
            write!(f, "{}: ", location_list.join(", "))?;
        }

        let action_str = match &self {
            Task {
                description: Some(description),
                ..
            } => description.clone(),
            Task {
                payload: Payload::String(payload),
                ..
            } => payload.clone(),
            Task {
                payload: Payload::Json(payload),
                ..
            } => command_to_text(payload),
        };

        write!(f, "{}", action_str)?;

        Ok(())
    }
}

/// The schedule with all values completed.
///
/// Note this is not used in the backend, which has its own copy.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Sequence {
    /// The id of the step.
    pub id: String,

    /// The name of this sequence.
    // #[serde(skip)]
    // pub sequence_name: String,

    /// The conditions that must be true before this is scheduled.
    // #[serde(skip)]
    // if_cond: Option<Vec<Boolean<Context>>>,

    /// The required classifications for this step.
    // #[serde(skip)]
    // classifications: Option<HashSet<String>>,

    // The required options for this step.
    // #[serde(skip)]
    // options: Option<HashSet<String>>,

    /// If true this is considered the "zero time" for this sequence.
    // #[serde(skip)]
    // zero_time: bool,

    /// The start time of this step.
    pub required_time: DateTime<Utc>,

    /// The required duration of this step.
    required_duration: Duration,

    /// The latest time this step can be completed.
    pub latest_time: DateTime<Utc>,

    /// The number of the repeat.
    repeat_number: usize,

    /// The tasks to execute.
    pub tasks: Vec<Task>,

    /// The mark for this task - for use by executor.
    pub mark: Option<Mark>,
}

// impl Ord for Sequence {
//     fn cmp(&self, other: &Self) -> std::cmp::Ordering {
//         self.required_time.cmp(&other.required_time)
//     }
// }

// impl PartialOrd for Sequence {
//     fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
//         self.required_time.partial_cmp(&other.required_time)
//     }
// }
// impl Eq for Sequence {}

// impl PartialEq for Sequence {
//     fn eq(&self, other: &Self) -> bool {
//         self.required_time == other.required_time
//     }
// }

/// The tags for yesterday, today, and tomorrow.
#[derive(Debug, Deserialize, Serialize)]
pub struct Tags {
    /// The tags for yesterday.
    pub yesterday: HashSet<String>,

    /// The tags for today.
    pub today: HashSet<String>,

    /// The tags for tomorrow.
    pub tomorrow: HashSet<String>,
}
