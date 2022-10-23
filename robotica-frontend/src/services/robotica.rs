//! Common structures to communicate with robotica
use serde::{Deserialize, Serialize};

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
