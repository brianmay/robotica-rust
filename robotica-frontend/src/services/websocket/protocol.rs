//! Common interfaces between robotica frontends and backends
use serde::{Deserialize, Serialize};

use crate::services::protocol::User;

#[derive(Debug, Deserialize)]
#[serde(tag = "error")]
pub(super) enum WsError {
    NotAuthorized,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(super) enum WsConnect {
    Connected(User),
    Disconnected(WsError),
}

/// Message sent from the frotnend to the backend.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum WsCommand {
    /// Frontend wants to subscribe to MQTT topic.
    Subscribe {
        /// MQTT topic to subscribe to.
        topic: String,
    },

    /// Frontend wants to send a MQTT message.
    Send(MqttMessage),

    /// Keep alive message.
    KeepAlive,
}

/// A MQTT message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MqttMessage {
    /// MQTT topic to send the message to.
    pub topic: String,

    /// MQTT message to send.
    pub payload: String,
}
