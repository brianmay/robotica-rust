//! Common structs shared between robotica-backend and robotica-frontend for websockets
use serde::{Deserialize, Serialize};

use crate::{user::User, version::Version};

/// Error message sent from the backend to the frontend
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "error")]
pub enum WsError {
    /// The user is not authorized to access the websocket
    NotAuthorized,
}

/// Message sent from the backend to the frontend after websocket connected
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum WsConnect {
    /// The websocket is connected
    Connected {
        /// The user that is connected
        user: User,

        /// The version of the backend
        version: Version,
    },

    /// The websocket is disconnected
    Disconnected(WsError),
}

/// Message sent from the frontend to the backend.
#[derive(Debug, Deserialize, Serialize)]
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
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MqttMessage {
    /// MQTT topic to send the message to.
    pub topic: String,

    /// MQTT message to send.
    pub payload: String,
}
