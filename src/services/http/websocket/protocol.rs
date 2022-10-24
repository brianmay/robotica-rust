use std::str::Utf8Error;

use serde::{Deserialize, Serialize};

use crate::services::mqtt;

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(super) enum WsCommand {
    Subscribe { topic: String },
    Send(MqttMessage),
    KeepAlive,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct MqttMessage {
    pub(super) topic: String,
    pub(super) payload: String,
}

impl From<MqttMessage> for mqtt::Message {
    fn from(msg: MqttMessage) -> Self {
        mqtt::Message::from_string(&msg.topic, &msg.payload, false, mqtt::QoS::exactly_once())
    }
}

impl TryFrom<mqtt::Message> for MqttMessage {
    type Error = Utf8Error;

    fn try_from(msg: mqtt::Message) -> Result<Self, Self::Error> {
        let payload = msg.payload_into_string()?;
        Ok(MqttMessage {
            topic: msg.topic,
            payload,
        })
    }
}
