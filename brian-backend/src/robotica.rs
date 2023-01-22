use log::info;
use robotica_backend::entities::create_stateless_entity;
use robotica_backend::entities::Sender;
use robotica_backend::services::mqtt::Mqtt;
use robotica_backend::services::mqtt::Subscriptions;
use robotica_common::mqtt::MqttMessage;
use robotica_common::mqtt::QoS;
use robotica_common::robotica::AudioCommand;
use robotica_common::robotica::Command;
use robotica_common::robotica::DevicePower;
use serde::Serialize;

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
    let msg = Command::Audio(AudioCommand {
        title: "Robotica Rust".into(),
        message: str.into(),
    });

    let payload = serde_json::to_string(&msg).unwrap();
    MqttMessage::new(topic, payload, false, QoS::ExactlyOnce)
}

pub fn create_message_sink(subscriptions: &mut Subscriptions, mqtt: Mqtt) -> Sender<String> {
    let gate_topic = Id::new("Brian", "Messages").get_state_topic("power");
    let gate_in = subscriptions.subscribe_into_stateless::<DevicePower>(&gate_topic);

    let night_topic = Id::new("Brian", "Night").get_state_topic("power");
    let night_in = subscriptions.subscribe_into_stateless::<DevicePower>(&night_topic);

    let (tx, rx) = create_stateless_entity::<String>("messages");
    tokio::spawn(async move {
        let mut rx = rx.subscribe().await;
        while let Ok(msg) = rx.recv().await {
            info!("Sending message {}", msg);

            let gate = gate_in.get().await == Some(DevicePower::On);
            let night = night_in.get().await == Some(DevicePower::On);

            if gate && !night {
                let msg = string_to_message(msg);
                mqtt.try_send(msg);
            }
        }
    });
    tx
}
