//! A Command and a recipient

use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::error;

use crate::mqtt::{self, MqttMessage};

use super::commands::Command;

/// Payload in a task.
#[derive(Debug, Clone, Deserialize, Serialize)]
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
    /// The payload of the task.
    #[serde(flatten)]
    pub payload: Payload,

    /// The qos to be used when sending the message.
    pub qos: mqtt::QoS,

    /// The retain value to be used when sending the message.
    pub retain: bool,

    /// The topics this task will send to.
    pub topics: Vec<String>,
}

impl Task {
    /// Get the MQTT message for this task.
    #[must_use]
    pub fn get_mqtt_payload(&self) -> Option<String> {
        match &self.payload {
            Payload::String(s) => Some(s.to_string()),
            Payload::Json(v) => Some(v.to_string()),
            Payload::Command(c) => serde_json::to_string(c)
                .map_err(|err| {
                    error!("Failed to serialize command: {:?}", c);
                    err
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubTask {
    /// The payload of the task.
    #[serde(flatten)]
    pub payload: Payload,

    /// The qos to be used when sending the message.
    pub qos: mqtt::QoS,

    /// The retain value to be used when sending the message.
    pub retain: bool,

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
        if let Some(location_str) = topics_to_locations(&self.topics) {
            write!(f, "{location_str}: ")?;
        }

        let action_str = match &self.payload {
            Payload::String(payload) => string_to_text(&self.topics, payload),
            Payload::Json(payload) => json_command_to_text(payload),
            Payload::Command(payload) => payload.to_string(),
        };

        write!(f, "{action_str}")?;
        Ok(())
    }
}

fn topics_to_locations(topics: &[String]) -> Option<String> {
    if topics.is_empty() {
        Some("Nowhere".to_string())
    } else {
        let locations = topics
            .iter()
            .filter_map(|s| topic_to_location(s))
            .collect::<Vec<_>>();

        if locations.is_empty() {
            None
        } else {
            Some(locations.join(", "))
        }
    }
}

fn topic_to_location(topic: &str) -> Option<String> {
    let split: Vec<&str> = topic.split('/').collect();

    #[allow(clippy::match_same_arms)]
    match split[..] {
        ["ha", "event", "message"] => None,
        ["ha", "event", "sleep", "brian", ..] => Some("Brian's Room".to_string()),
        ["ha", "event", "wakeup", "brian", ..] => Some("Brian's Room".to_string()),
        ["command", "Brian", ..] => Some("Brian's Room".to_string()),
        ["command", "Jan", ..] => Some("Jan's Room".to_string()),
        ["command", "Twins", ..] => Some("Twins' Room".to_string()),
        ["command", "Tesla", ..] => Some("Tesla".to_string()),
        _ => Some(topic.to_string()),
    }
}

fn string_to_text(topics: &[String], payload: &str) -> String {
    let topic = topics.first().map(String::as_str);

    match topic {
        Some("ha/event/sleep/brian/1") => "Sleep 1".to_string(),
        Some("ha/event/sleep/brian/2") => "Sleep 2".to_string(),
        Some("ha/event/sleep/brian/3") => "Sleep 3".to_string(),
        Some("ha/event/sleep/brian/4") => "Sleep 4".to_string(),
        Some("ha/event/wakeup/brian/1") => "Wake up 1".to_string(),
        Some("ha/event/wakeup/brian/2") => "Wake up 2".to_string(),
        Some("ha/event/wakeup/brian/3") => "Wake up 3".to_string(),
        Some("ha/event/wakeup/brian/4") => "Wake up 4".to_string(),
        _ => format!("Unknown {} {}", topics.join(", "), payload),
    }
}
