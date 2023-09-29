use robotica_backend::pipes::stateless;
use robotica_backend::pipes::Subscriber;
use robotica_backend::pipes::Subscription;
use robotica_backend::services::mqtt::MqttTx;
use robotica_common::mqtt::Json;
use robotica_common::mqtt::MqttMessage;
use robotica_common::mqtt::MqttSerializer;
use robotica_common::mqtt::QoS;
use robotica_common::robotica::commands::Command;
use robotica_common::robotica::message::Message;
use robotica_common::robotica::message::MessageAudience;
use tracing::error;
use tracing::info;

pub fn message_topic(msg: &Message) -> String {
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
fn string_to_message(msg: Message) -> Option<MqttMessage> {
    let topic = message_topic(&msg);
    let command = Command::Message(msg);
    Json(command)
        .serialize(topic, false, QoS::ExactlyOnce)
        .map_err(|err| {
            error!("Failed to serialize message.");
            err
        })
        .ok()
}

pub fn create_message_sink(mqtt: MqttTx) -> stateless::Sender<Message> {
    let (tx, rx) = stateless::create_pipe::<Message>("messages");
    tokio::spawn(async move {
        let mut rx = rx.subscribe().await;
        while let Ok(msg) = rx.recv().await {
            info!("Sending message {:?}", msg);
            if let Some(msg) = string_to_message(msg) {
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
    use robotica_common::robotica::audio::MessagePriority;
    use serde_json::json;

    #[test]
    fn test_message_command() {
        let message = Message {
            title: "Title".to_string(),
            body: "Body".to_string(),
            priority: MessagePriority::Low,
            audience: MessageAudience::Everyone,
            flash_lights: false,
        };
        let json = json!({
            "title": "Title",
            "body": "Body",
            "priority": "Low",
            "audience": "Everyone",
            "flash_lights": false,
        });
        assert_eq!(json, serde_json::to_value(message).unwrap());
    }
}
