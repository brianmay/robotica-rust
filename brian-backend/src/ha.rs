use robotica_backend::pipes::stateless;
use robotica_backend::services::mqtt::MqttTx;
use robotica_backend::services::mqtt::SendOptions;
use robotica_common::robotica::message::Message;

pub fn create_message_sink(mqtt: &MqttTx) -> stateless::Sender<Message> {
    let (tx, rx) = stateless::create_pipe::<Message>("messages");
    rx.send_to_mqtt_json(mqtt, "ha/event/message", &SendOptions::default());
    tx
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use robotica_common::robotica::{audio::MessagePriority, message::Audience};
    use serde_json::json;

    #[test]
    fn test_message_command() {
        let message = Message {
            title: "Title".to_string(),
            body: "Body".to_string(),
            priority: MessagePriority::Low,
            audience: Audience::new("everyone"),
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
