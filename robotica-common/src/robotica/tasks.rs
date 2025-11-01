//! A Command and a recipient

use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::error;

use crate::mqtt::{self, MqttMessage, Retain};

use super::commands::Command;

/// Payload in a task.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum Payload {
    /// A string payload.
    #[serde(rename = "payload_str")]
    String(String),

    /// A JSON payload.
    #[serde(rename = "payload_json")]
    Json(Value),

    /// A Robotica command.
    #[serde(rename = "payload_command")]
    Command(Command),
}

/// A task with all values completed.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Task {
    /// The title of the task.
    pub title: String,

    /// The payload of the task.
    #[serde(flatten)]
    pub payload: Payload,

    /// The qos to be used when sending the message.
    pub qos: mqtt::QoS,

    /// The retain value to be used when sending the message.
    pub retain: mqtt::Retain,

    /// The topics this task will send to.
    pub topics: Vec<String>,
}

impl Task {
    /// Get the MQTT message for this task.
    #[must_use]
    pub fn get_mqtt_payload(&self) -> Option<String> {
        match &self.payload {
            Payload::String(s) => Some(s.clone()),
            Payload::Json(v) => Some(v.to_string()),
            Payload::Command(c) => serde_json::to_string(c)
                .inspect_err(|err| {
                    error!("Failed to serialize command {c:?}: {err:?}");
                })
                .ok(),
        }
    }

    /// Get the MQTT messages for this task.
    #[must_use]
    pub fn get_mqtt_messages(&self) -> Vec<MqttMessage> {
        let maybe_payload = self.get_mqtt_payload();
        maybe_payload.map_or_else(Vec::new, |payload| {
            self.topics
                .iter()
                .map(|topic| MqttMessage::new(topic, payload.clone(), self.retain, self.qos))
                .collect()
        })
    }
}

/// A task with optional target names instead of topics.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SubTask {
    /// The with a target instead of the required topic.
    pub title: String,

    /// The payload of the task.
    #[serde(flatten)]
    pub payload: Payload,

    /// The qos to be used when sending the message.
    pub qos: mqtt::QoS,

    /// The retain value to be used when sending the message.
    pub retain: Retain,

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
            title: self.title,
            payload: self.payload,
            qos: self.qos,
            retain: self.retain,
            topics,
        }
    }
}

fn json_command_to_text(command: &Value) -> String {
    match serde_json::from_value::<Command>(command.clone()) {
        Ok(command) => command.to_string(),
        Err(err) => {
            format!("Error parsing command: {err}")
        }
    }
}

impl Display for Task {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let action_str = match &self.payload {
            Payload::String(payload) => payload.clone(),
            Payload::Json(payload) => json_command_to_text(payload),
            Payload::Command(payload) => payload.to_string(),
        };

        write!(f, "{action_str}")?;

        Ok(())
    }
}
