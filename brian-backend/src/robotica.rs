use robotica_backend::entities::create_stateless_entity;
use robotica_backend::entities::Sender;
use robotica_backend::services::mqtt::MqttTx;
use robotica_common::mqtt::MqttMessage;
use robotica_common::mqtt::QoS;
use robotica_common::robotica::commands::AudioCommand;
use robotica_common::robotica::commands::Command;
use robotica_common::robotica::commands::DevicePower;
use serde::Serialize;
use tracing::error;
use tracing::info;

#[derive(Clone)]
pub struct Id {
    pub location: String,
    pub device: String,
}

impl Id {
    pub fn new(location: &str, device: &str) -> Self {
        Self {
            location: location.to_string(),
            device: device.to_string(),
        }
    }
    pub fn get_name(&self, postfix: &str) -> String {
        format!("{}_{}_{postfix}", self.location, self.device)
    }
    pub fn get_state_topic(&self, component: &str) -> String {
        format!("state/{}/{}/{component}", self.location, self.device)
    }
    pub fn get_command_topic(&self, params: &[&str]) -> String {
        match params.join("/").as_str() {
            "" => format!("command/{}/{}", self.location, self.device),
            params => format!("command/{}/{}/{params}", self.location, self.device),
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AutoColorOut {
    pub hue: Option<u16>,
    pub saturation: Option<u16>,
    pub brightness: Option<u16>,
    pub kelvin: Option<u16>,
    pub alpha: Option<u16>,
}

#[derive(Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
struct AutoColor {
    pub power: Option<DevicePower>,
    pub color: AutoColorOut,
}

#[allow(dead_code)]
pub fn string_to_message(str: impl Into<String>) -> MqttMessage {
    let topic = "ha/event/message/everyone";
    let msg = Command::Audio(AudioCommand::new_message("Robotica Rust", str));

    let payload = serde_json::to_string(&msg).unwrap_or_else(|_| {
        error!("Failed to serialize message: {msg:?}");
        "{}".into()
    });
    MqttMessage::new(topic, payload, false, QoS::ExactlyOnce)
}

pub fn create_message_sink(mqtt: MqttTx) -> Sender<String> {
    let (tx, rx) = create_stateless_entity::<String>("messages");
    tokio::spawn(async move {
        let mut rx = rx.subscribe().await;
        while let Ok(msg) = rx.recv().await {
            info!("Sending message {}", msg);
            let msg = string_to_message(msg);
            mqtt.try_send(msg);
        }
    });
    tx
}
