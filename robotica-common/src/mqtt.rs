//! Common Mqtt stuff

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(feature = "websockets")]
use crate::{protobuf::ProtobufIntoFrom, protos};

/// The `QoS` level for a MQTT message.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum QoS {
    /// At most once
    AtMostOnce = 0,

    /// At least once
    AtLeastOnce = 1,

    /// Exactly once
    ExactlyOnce = 2,
}

impl Serialize for QoS {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            QoS::AtMostOnce => serializer.serialize_u8(0),
            QoS::AtLeastOnce => serializer.serialize_u8(1),
            QoS::ExactlyOnce => serializer.serialize_u8(2),
        }
    }
}

impl<'de> Deserialize<'de> for QoS {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        match value {
            0 => Ok(QoS::AtMostOnce),
            1 => Ok(QoS::AtLeastOnce),
            2 => Ok(QoS::ExactlyOnce),
            _ => Err(serde::de::Error::custom(format!(
                "Invalid QoS value: {}",
                value
            ))),
        }
    }
}

/// A MQTT message.
#[derive(Clone, Debug)]
pub struct MqttMessage {
    /// MQTT topic to send the message to.
    pub topic: String,

    /// MQTT message to send.
    pub payload: String,

    /// Was/Is this message retained?
    pub retain: bool,

    /// What is the QoS of this message?
    pub qos: QoS,
}

impl Default for MqttMessage {
    fn default() -> Self {
        Self {
            topic: String::new(),
            payload: String::new(),
            retain: false,
            qos: QoS::ExactlyOnce,
        }
    }
}

#[cfg(feature = "websockets")]
impl ProtobufIntoFrom for MqttMessage {
    type Protobuf = protos::EncodedMqttMessage;

    fn into_protobuf(self) -> Self::Protobuf {
        Self::Protobuf {
            topic: self.topic,
            payload: self.payload,
            retain: self.retain,
            qos: self.qos as u32,
        }
    }

    fn from_protobuf(msg: Self::Protobuf) -> Option<Self> {
        Some(Self {
            topic: msg.topic,
            payload: msg.payload,
            retain: msg.retain,
            qos: match msg.qos {
                0 => QoS::AtMostOnce,
                1 => QoS::AtLeastOnce,
                _ => QoS::ExactlyOnce,
            },
        })
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
    pub fn new(
        topic: impl Into<String>,
        payload: impl Into<String>,
        retain: bool,
        qos: QoS,
    ) -> Self {
        Self {
            topic: topic.into(),
            payload: payload.into(),
            retain,
            qos,
        }
    }

    /// Create a new message from a json payload.
    ///
    /// # Errors
    ///
    /// This function will fail if the payload cannot be serialized to json.
    pub fn from_json(
        topic: impl Into<String>,
        payload: &serde_json::Value,
        retain: bool,
        qos: QoS,
    ) -> Result<Self, serde_json::Error> {
        let payload = serde_json::to_string(payload)?;
        Ok(Self::new(topic, payload, retain, qos))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_serialize_qos() {
        let qos = QoS::AtMostOnce;
        let serialized = serde_json::to_string(&qos).unwrap();
        assert_eq!(serialized, "0");

        let qos = QoS::AtLeastOnce;
        let serialized = serde_json::to_string(&qos).unwrap();
        assert_eq!(serialized, "1");

        let qos = QoS::ExactlyOnce;
        let serialized = serde_json::to_string(&qos).unwrap();
        assert_eq!(serialized, "2");
    }

    #[test]
    fn test_deserialize_qos() {
        let qos: QoS = serde_json::from_str("0").unwrap();
        assert_eq!(qos, QoS::AtMostOnce);

        let qos: QoS = serde_json::from_str("1").unwrap();
        assert_eq!(qos, QoS::AtLeastOnce);

        let qos: QoS = serde_json::from_str("2").unwrap();
        assert_eq!(qos, QoS::ExactlyOnce);

        let err: Result<QoS, _> = serde_json::from_str("3");
        assert!(err.is_err());
    }
}
