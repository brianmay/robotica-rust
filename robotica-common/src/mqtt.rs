//! Common Mqtt stuff

use core::fmt;
use std::{
    convert::Infallible,
    error::Error,
    fmt::Formatter,
    ops::Deref,
    str::{FromStr, Utf8Error},
    string::FromUtf8Error,
    sync::Arc,
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

#[cfg(feature = "websockets")]
use crate::{protobuf::ProtobufIntoFrom, protos};

/// The retain flag for a MQTT message.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Retain {
    /// The message should be retained.
    Retain,

    /// The message should not be retained.
    NoRetain,
}

impl Serialize for Retain {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Retain::Retain => serializer.serialize_bool(true),
            Retain::NoRetain => serializer.serialize_bool(false),
        }
    }
}

impl<'de> Deserialize<'de> for Retain {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = bool::deserialize(deserializer)?;
        if value {
            Ok(Retain::Retain)
        } else {
            Ok(Retain::NoRetain)
        }
    }
}

/// The `QoS` level for a MQTT message.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
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
                "Invalid QoS value: {value}"
            ))),
        }
    }
}

/// A MQTT message.
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct MqttMessage {
    /// MQTT topic to send the message to.
    pub topic: String,

    /// MQTT message to send.
    pub payload: Vec<u8>,

    /// Was/Is this message retained?
    pub retain: Retain,

    /// What is the `QoS` of this message?
    pub qos: QoS,
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

impl std::fmt::Debug for MqttMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let payload = String::from_utf8(self.payload.clone())
            .unwrap_or_else(|_| format!("{:?}", self.payload));
        let payload = truncate(&payload, 40);

        f.debug_struct("MqttMessage")
            .field("topic", &self.topic)
            .field("payload", &payload)
            .field("retain", &self.retain)
            .field("qos", &self.qos)
            .finish()
    }
}

impl Default for MqttMessage {
    fn default() -> Self {
        Self {
            topic: String::new(),
            payload: Vec::new(),
            retain: Retain::NoRetain,
            qos: QoS::ExactlyOnce,
        }
    }
}

impl MqttMessage {
    /// Create a new message.
    #[must_use]
    pub fn new(
        topic: impl Into<String>,
        payload: impl Into<String>,
        retain: Retain,
        qos: QoS,
    ) -> Self {
        Self {
            topic: topic.into(),
            payload: payload.into().bytes().collect(),
            retain,
            qos,
        }
    }

    /// Create a new message from a Mqtt Serializer
    ///
    /// # Errors
    ///
    /// If the payload cannot be serialized.
    pub fn from_serializer<T: MqttSerializer>(
        topic: impl Into<String>,
        payload: &T,
        retain: Retain,
        qos: QoS,
    ) -> Result<Self, T::Error> {
        payload.serialize(topic, retain, qos)
    }

    /// Create a new message from a JSON value.
    ///
    /// # Errors
    ///
    /// If the JSON value cannot be serialized.
    pub fn from_json(
        topic: impl Into<String>,
        payload: &impl Serialize,
        retain: Retain,
        qos: QoS,
    ) -> Result<Self, serde_json::Error> {
        let payload = serde_json::to_string(payload)?;
        Ok(Self::new(topic, payload, retain, qos))
    }

    /// Return reference to decoded string payload.
    ///
    /// # Errors
    ///
    /// If the payload is not valid UTF-8.
    #[allow(clippy::missing_const_for_fn)]
    pub fn payload_as_str(&self) -> Result<&str, Utf8Error> {
        std::str::from_utf8(&self.payload)
    }
}

#[cfg(feature = "websockets")]
impl ProtobufIntoFrom for MqttMessage {
    type Protobuf = protos::EncodedMqttMessage;

    fn into_protobuf(self) -> Self::Protobuf {
        Self::Protobuf {
            topic: self.topic,
            payload: self.payload,
            retain: match self.retain {
                Retain::Retain => true,
                Retain::NoRetain => false,
            },
            qos: self.qos as u32,
        }
    }

    fn from_protobuf(msg: Self::Protobuf) -> Option<Self> {
        Some(Self {
            topic: msg.topic,
            payload: msg.payload,
            retain: if msg.retain {
                Retain::Retain
            } else {
                Retain::NoRetain
            },
            qos: match msg.qos {
                0 => QoS::AtMostOnce,
                1 => QoS::AtLeastOnce,
                _ => QoS::ExactlyOnce,
            },
        })
    }
}

impl TryFrom<MqttMessage> for String {
    type Error = FromUtf8Error;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        let payload = String::from_utf8(msg.payload)?;
        Ok(payload)
    }
}

/// The error type for bool conversion
#[derive(Error, Debug)]
pub enum BoolError {
    /// The payload was not a valid boolean string.
    #[error("Invalid value: {0}")]
    InvalidValue(String),

    /// The payload was not a valid UTF-8 string.
    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] Utf8Error),
}

impl TryFrom<MqttMessage> for bool {
    type Error = BoolError;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        let payload: &str = msg.payload_as_str()?;
        match payload {
            "true" => Ok(true),
            "false" => Ok(false),
            value => Err(BoolError::InvalidValue(value.to_string())),
        }
    }
}

/// A parsed MQTT message.
pub struct Parsed<Body: FromStr>(pub Body);

impl<T: FromStr + Clone> Clone for Parsed<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: FromStr + PartialEq> PartialEq for Parsed<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl<T: FromStr + Eq> Eq for Parsed<T> {}

impl<T: FromStr> Deref for Parsed<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The error type for u8 conversion
#[derive(Error, Debug)]
pub enum ParsedError<ParserError> {
    /// The payload could not be parsed.
    #[error("Invalid value: {0}")]
    InvalidValue(ParserError),

    /// The payload was not a valid UTF-8 string.
    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] Utf8Error),
}

impl<Body: FromStr> TryFrom<MqttMessage> for Parsed<Body> {
    type Error = ParsedError<Body::Err>;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        let payload: &str = msg.payload_as_str()?;
        let payload: Body = payload.parse().map_err(ParsedError::InvalidValue)?;
        Ok(Parsed(payload))
    }
}

/// A JSON MQTT message.
pub struct Json<T>(pub T);

impl<T> Json<T> {
    /// Unwrap the inner value
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Clone> Clone for Json<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Json<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: std::fmt::Display> std::fmt::Display for Json<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: PartialEq> PartialEq for Json<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl<T> Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Eq> Eq for Json<T> {}

/// The error type for JSON conversion
#[derive(Error, Debug)]
pub enum JsonError {
    /// The payload was not a valid JSON string.
    #[error("Invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),

    /// The payload was not a valid UTF-8 string.
    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] Utf8Error),
}

impl<Body: DeserializeOwned> TryFrom<MqttMessage> for Json<Body> {
    type Error = JsonError;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        let payload: &str = msg.payload_as_str()?;
        let value = serde_json::from_str(payload)?;
        Ok(Json(value))
    }
}

impl<Body: DeserializeOwned> TryFrom<MqttMessage> for Arc<Json<Body>> {
    type Error = JsonError;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        let payload: &str = msg.payload_as_str()?;
        let value = serde_json::from_str(payload)?;
        Ok(Arc::new(Json(value)))
    }
}

/// Serialize an object to a MQTT message.
pub trait MqttSerializer {
    /// The error type for serialization.
    type Error: Error;

    /// Serialize an object to a MQTT message.
    ///
    /// # Errors
    ///
    /// If the object cannot be serialized.
    fn serialize(
        &self,
        topic: impl Into<String>,
        retain: Retain,
        qos: QoS,
    ) -> Result<MqttMessage, Self::Error>;
}

impl<T: Serialize> MqttSerializer for Json<T> {
    type Error = JsonError;

    fn serialize(
        &self,
        topic: impl Into<String>,
        retain: Retain,
        qos: QoS,
    ) -> Result<MqttMessage, Self::Error> {
        let payload = serde_json::to_string(&self.0)?;
        Ok(MqttMessage::new(topic, payload, retain, qos))
    }
}

impl MqttSerializer for String {
    type Error = Infallible;
    fn serialize(
        &self,
        topic: impl Into<String>,
        retain: Retain,
        qos: QoS,
    ) -> Result<MqttMessage, Self::Error> {
        Ok(MqttMessage::new(topic, self, retain, qos))
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
