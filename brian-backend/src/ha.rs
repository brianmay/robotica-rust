use monostate::MustBe;
use robotica_backend::pipes::stateless;
use robotica_backend::pipes::Subscriber;
use robotica_backend::pipes::Subscription;
use robotica_backend::services::mqtt::MqttTx;
use robotica_common::mqtt::Json;
use robotica_common::mqtt::MqttMessage;
use robotica_common::mqtt::MqttSerializer;
use robotica_common::mqtt::QoS;
use robotica_common::robotica::audio::MessagePriority;
use serde::Serialize;
use tracing::error;
use tracing::info;

/// The audience of a message
#[derive(Debug, Clone, Serialize, Default)]
#[allow(dead_code)]
pub enum MessageAudience {
    /// Message goes to everyone.
    #[default]
    Everyone,

    /// Message goes to Brian only.
    Brian {
        /// Message is private for Brian only
        private: bool,
    },

    /// Message goes to the twins only.
    Twins,
}

/// A HA audio command
#[derive(Debug, Clone, Serialize)]
pub struct MessageCommand {
    /// The type of the command
    #[serde(rename = "type")]
    pub cmd_type: MustBe!("message"),

    /// The title of the message.
    pub title: String,

    /// The message to send.
    pub body: String,

    /// The priority of the message
    pub priority: MessagePriority,

    /// The audience of the message
    #[serde(default)]
    pub audience: MessageAudience,
}

impl MessageCommand {
    /// Create a new message
    pub fn new(
        title: impl Into<String>,
        body: impl Into<String>,
        priority: MessagePriority,
        audience: MessageAudience,
    ) -> Self {
        Self {
            cmd_type: monostate::MustBeStr,
            title: title.into(),
            body: body.into(),
            priority,
            audience,
        }
    }
}

pub fn message_topic(msg: &MessageCommand) -> String {
    match msg.audience {
        MessageAudience::Everyone => "ha/event/message/everyone".into(),
        MessageAudience::Brian { private } => {
            if private {
                "ha/event/message/brian/private".into()
            } else {
                "ha/event/message/brian".into()
            }
        }
        MessageAudience::Twins => "ha/event/message/twins".into(),
    }
}

#[allow(dead_code)]
fn string_to_message(msg: &MessageCommand) -> Option<MqttMessage> {
    let topic = message_topic(msg);
    Json(msg)
        .serialize(topic, false, QoS::ExactlyOnce)
        .map_err(|err| {
            error!("Failed to serialize message: {msg:?}");
            err
        }).ok()
}

pub fn create_message_sink(mqtt: MqttTx) -> stateless::Sender<MessageCommand> {
    let (tx, rx) = stateless::create_pipe::<MessageCommand>("messages");
    tokio::spawn(async move {
        let mut rx = rx.subscribe().await;
        while let Ok(msg) = rx.recv().await {
            info!("Sending message {:?}", msg);
            if let Some(msg) = string_to_message(&msg) {
                mqtt.try_send(msg);
            }
        }
    });
    tx
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use serde_json::json;

    #[test]
    fn test_ha_message_command() {
        let command = MessageCommand {
            cmd_type: monostate::MustBeStr,
            title: "Title".to_string(),
            body: "Body".to_string(),
            priority: MessagePriority::Low,
            audience: MessageAudience::Everyone,
        };
        let json = json!({
            "type": "message",
            "title": "Title",
            "body": "Body",
            "priority": "Low",
            "audience": "Everyone",
        });
        assert_eq!(json, serde_json::to_value(command).unwrap());
    }
}
