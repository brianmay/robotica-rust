//! Common interfaces between robotica frontends and backends
use std::{
    collections::HashSet,
    fmt::{Display, Formatter},
};

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;

use crate::types::{DateTime, Mark};

/// The user on the backend
#[derive(Clone, Debug, Deserialize)]
pub struct User {
    /// The name of the user on the backend
    pub name: String,
}

impl Display for User {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Payload in a task.
#[derive(Deserialize, Debug, Clone)]
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
#[derive(Debug, Clone, Deserialize)]
pub struct Task {
    /// The description of the task.
    pub description: Option<String>,

    /// The payload of the task.
    #[serde(flatten)]
    pub payload: Payload,

    /// The locations this task acts on.
    locations: Vec<String>,

    /// The devices this task acts on.
    devices: Vec<String>,

    /// The topics this task will send to.
    ///
    /// If this is not specified, the topic will be generated from the locations and devices.
    topics: Vec<String>,
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
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Sequence {
    /// The id of the step.
    pub id: String,

    /// The start time of this step.
    pub required_time: DateTime<Utc>,

    /// The latest time this step can be completed.
    pub latest_time: DateTime<Utc>,

    /// The number of the repeat.
    repeat_number: usize,

    /// The tasks to execute.
    pub tasks: Vec<Task>,

    /// The mark for this task - for use by executor.
    pub mark: Option<Mark>,
}

/// The tags for yesterday, today, and tomorrow.
#[derive(Debug, Deserialize)]
pub struct Tags {
    /// The tags for yesterday.
    pub yesterday: HashSet<String>,

    /// The tags for today.
    pub today: HashSet<String>,

    /// The tags for tomorrow.
    pub tomorrow: HashSet<String>,
}
