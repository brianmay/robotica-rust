use robotica_backend::pipes::stateless;
use robotica_backend::pipes::Subscriber;
use robotica_backend::pipes::Subscription;
use robotica_backend::services::mqtt::MqttTx;
use robotica_common::mqtt::Json;
use robotica_common::mqtt::QoS;
use robotica_common::mqtt::Retain;
use robotica_common::robotica::commands::Command;
use robotica_common::robotica::message::Message;
use tracing::info;

pub fn create_message_sink(mqtt: MqttTx) -> stateless::Sender<Message> {
    let (tx, rx) = stateless::create_pipe::<Message>("messages");
    tokio::spawn(async move {
        let mut rx = rx.subscribe().await;
        while let Ok(msg) = rx.recv().await {
            info!("Sending message {:?}", msg);
            let payload = Json(Command::Message(msg));
            mqtt.try_serialize_send(
                "ha/event/message",
                &payload,
                Retain::NoRetain,
                QoS::ExactlyOnce,
            );
        }
    });
    tx
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use crate::audience;

    use super::*;
    use robotica_common::robotica::audio::MessagePriority;
    use serde_json::json;

    #[test]
    fn test_message_command() {
        let message = Message {
            title: "Title".to_string(),
            body: "Body".to_string(),
            priority: MessagePriority::Low,
            audience: audience::everyone().to_string(),
            flash_lights: false,
        };
        let json = json!({
            "title": "Title",
            "body": "Body",
            "priority": "Low",
            "audience": "everyone",
            "flash_lights": false,
        });
        assert_eq!(json, serde_json::to_value(message).unwrap());
    }
}
