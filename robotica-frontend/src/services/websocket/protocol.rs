//! Common structures to communicate with robotica
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
    Disconnected,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum WsCommand {
    Subscribe { topic: String },
    Send(MqttMessage),
    KeepAlive,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
}
