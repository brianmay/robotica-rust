use std::fmt::Display;

use log::info;
use robotica_node_rust::entities::create_stateless_entity;
use robotica_node_rust::entities::Sender;
use robotica_node_rust::sources::mqtt::Message;
use robotica_node_rust::sources::mqtt::MqttOut;
use robotica_node_rust::sources::mqtt::QoS;
use robotica_node_rust::sources::mqtt::Subscriptions;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone)]
pub struct Id {
    pub location: String,
    pub device: String,
}

impl Id {
    pub fn new(location: &str, device: &str) -> Id {
        Id {
            location: location.to_string(),
            device: device.to_string(),
        }
    }
    // pub fn get_name(&self, postfix: &str) -> String {
    //     format!("{}_{}_{}", self.location, self.device, postfix)
    // }
    pub fn get_state_topic(&self, component: &str) -> String {
        format!("state/{}/{}/{}", self.location, self.device, component)
    }
    pub fn get_command_topic(&self, params: &[&str]) -> String {
        match params.join("/").as_str() {
            "" => format!("command/{}/{}", self.location, self.device),
            params => format!("command/{}/{}/{}", self.location, self.device, params),
        }
    }
}

#[derive(Clone, Error, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Power {
    On,
    Off,
    HardOff,
}

impl Display for Power {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Power::On => write!(f, "on"),
            Power::Off => write!(f, "off"),
            Power::HardOff => write!(f, "hard off"),
        }
    }
}

#[derive(Error, Debug)]
pub enum PowerErr {
    #[error("Invalid power state: {0}")]
    InvalidPowerState(String),

    #[error("Invalid UTF8")]
    Utf8Error(#[from] std::str::Utf8Error),
}

impl TryFrom<Message> for Power {
    type Error = PowerErr;

    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        let payload: String = msg.try_into()?;
        match payload.as_str() {
            "ON" => Ok(Power::On),
            "OFF" => Ok(Power::Off),
            "HARD_OFF" => Ok(Power::HardOff),
            _ => Err(PowerErr::InvalidPowerState(payload)),
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoboticaColorOut {
    pub hue: u16,
    pub saturation: u16,
    pub brightness: u16,
    pub kelvin: u16,
}

#[allow(dead_code)]
#[derive(Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    TurnOn,
    TurnOff,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RoboticaLightCommand {
    pub action: Option<Action>,
    pub color: Option<RoboticaColorOut>,
    pub scene: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RoboticaDeviceCommand {
    pub action: Option<Action>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoboticaAutoColorOut {
    pub hue: Option<u16>,
    pub saturation: Option<u16>,
    pub brightness: Option<u16>,
    pub kelvin: Option<u16>,
    pub alpha: Option<u16>,
}

#[derive(Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoboticaAutoColor {
    pub power: Option<Power>,
    pub color: RoboticaAutoColorOut,
}

#[derive(Serialize, Deserialize, Debug)]
struct AudioMessage {
    #[serde(rename = "type")]
    msg_type: String,
    message: String,
}

#[allow(dead_code)]
pub fn string_to_message(str: &str, location: &str) -> Message {
    let id = Id::new(location, "Robotica");

    let msg = AudioMessage {
        msg_type: "audio".to_string(),
        message: str.to_string(),
    };
    let payload = serde_json::to_string(&msg).unwrap();
    Message::from_string(
        &id.get_command_topic(&[]),
        &payload,
        false,
        QoS::exactly_once(),
    )
}

pub(crate) fn create_message_sink(
    subscriptions: &mut Subscriptions,
    mqtt_out: MqttOut,
) -> Sender<String> {
    let gate_topic = Id::new("Brian", "Messages").get_state_topic("power");
    let gate_in = subscriptions.subscribe_into_stateless::<Power>(&gate_topic);

    let (tx, rx) = create_stateless_entity::<String>("messages");
    tokio::spawn(async move {
        let mut rx = rx.subscribe().await;
        while let Ok(msg) = rx.recv().await {
            info!("Sending message {}", msg);

            if let Some(Power::On) = gate_in.get().await {
                let msg = string_to_message(&msg, "Brian");
                mqtt_out.send(msg);
            }
        }
    });
    tx
}
