//! Common Mqtt stuff
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::datetime::{utc_now, DateTime};

/// The `QoS` level for a MQTT message.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Eq, PartialEq)]
pub enum QoS {
    /// At most once
    AtMostOnce,

    /// At least once
    AtLeastOnce,

    /// Exactly once
    ExactlyOnce,
}

/// A MQTT message.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MqttMessage {
    /// MQTT topic to send the message to.
    pub topic: String,

    /// MQTT message to send.
    pub payload: String,

    /// Was/Is this message retained?
    pub retain: bool,

    /// What is the QoS of this message?
    pub qos: QoS,

    /// What was the instant this message was created?
    pub instant: DateTime<Utc>,
}

impl Default for MqttMessage {
    fn default() -> Self {
        Self {
            topic: String::new(),
            payload: String::new(),
            retain: false,
            qos: QoS::ExactlyOnce,
            instant: utc_now(),
        }
    }
}

/// The error type for bool conversion
#[derive(Error, Debug)]
pub enum BoolError {
    /// The payload was not a valid boolean string.
    #[error("Invalid value: {0}")]
    InvalidValue(String),
}

impl TryFrom<MqttMessage> for bool {
    type Error = BoolError;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        let payload: String = msg.payload;
        match payload.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            value => Err(BoolError::InvalidValue(value.to_string())),
        }
    }
}

impl TryFrom<MqttMessage> for serde_json::Value {
    type Error = serde_json::Error;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        serde_json::from_str(&msg.payload)
    }
}

/// Dummy error for String conversion
#[derive(Error, Debug)]
pub enum StringError {}

impl TryFrom<MqttMessage> for String {
    type Error = StringError;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        Ok(msg.payload)
    }
}

impl MqttMessage {
    /// Create a new message.
    #[must_use]
    pub fn new(topic: &str, payload: String, retain: bool, qos: QoS) -> Self {
        Self {
            topic: topic.to_string(),
            payload,
            retain,
            qos,
            instant: utc_now(),
        }
    }
}
