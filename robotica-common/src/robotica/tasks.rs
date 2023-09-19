//! A Command and a recipient

use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::error;

use crate::mqtt;

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

/// A task with all values completed.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubTask {
    /// The description of the task.
    pub description: Option<String>,

    /// The payload of the task.
    #[serde(flatten)]
    pub payload: Payload,

    /// The qos to be used when sending the message.
    pub qos: Option<mqtt::QoS>,

    /// The retain value to be used when sending the message.
    pub retain: Option<bool>,

    /// The target of this subtask
    pub target: String,
}

impl SubTask {
    /// Convert `SubTask` to a `Task`
    #[must_use]
    pub fn to_task(self, targets: &HashMap<String, String>) -> Task {
        let topics = targets.get(&self.target).map_or_else(
            || {
                error!("Target {} not found", self.target);
                vec![]
            },
            |target| vec![target.clone()],
        );

        Task {
            description: self.description,
            payload: self.payload,
            qos: self.qos.unwrap_or(mqtt::QoS::ExactlyOnce),
            retain: self.retain.unwrap_or(false),
            locations: vec![],
            devices: vec![],
            topics,
        }
    }
}
fn location_device_to_text(location: &str, device: &str) -> String {
    format!("{location}/{device}")
}

fn command_to_text(command: &Value) -> String {
    let mtype = command.get("type").and_then(Value::as_str);

    match mtype {
        Some("audio") => audio_command_to_text(command),
        Some("message") => message_command_to_text(command),
        Some("light") => light_command_to_text(command),
        Some("device") => device_command_to_text(command),
        Some("hdmi") => hdmi_command_to_text(command),
        _ => command.to_string(),
    }
}

fn message_command_to_text(command: &Value) -> String {
    let message = command.get("body").and_then(Value::as_str);
    message.map_or_else(
        || "unknown message command".to_string(),
        |message| format!("say {message}"),
    )
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
        results.push(format!("set message volume {volume}"));
    }
    if let Some(message) = message {
        results.push(format!("say {message}"));
    }
    if let Some(music_stop) = music_stop {
        if music_stop {
            results.push("stop music".to_string());
        }
    }
    if let Some(volume) = music_volume {
        results.push(format!("set music volume {volume}"));
    }
    if let Some(play_list) = play_list {
        results.push(format!("play {play_list}"));
    }
    if results.is_empty() {
        "unknown audio command".to_string()
    } else {
        results.join(", ")
    }
}

fn light_command_to_text(command: &Value) -> String {
    let action = command.get("action").and_then(Value::as_str);
    let scene = command.get("scene").and_then(Value::as_str);

    match (action, scene) {
        (Some(command), Some(scene)) => format!("{command} scene {scene}"),
        (Some(command), None) => command.to_string(),
        (None, Some(scene)) => format!("turn_on scene {scene}"),
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

        write!(f, "{action_str}")?;

        Ok(())
    }
}
