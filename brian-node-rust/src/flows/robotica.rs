use paho_mqtt::Message;
use serde::{Deserialize, Serialize};

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
    pub fn get_state_topic(&self, component: &str) -> String {
        format!("state/{}/{}/{}", self.location, self.device, component)
    }
    pub fn get_google_out_topic(&self) -> String {
        format!("google/{}/{}/out", self.location, self.device)
    }
    pub fn get_google_in_topic(&self) -> String {
        format!("google/{}/{}/in", self.location, self.device)
    }
    pub fn get_command_topic(&self, params: &[&str]) -> String {
        match params.join("/").as_str() {
            "" => format!("command/{}/{}", self.location, self.device),
            params => format!("command/{}/{}/{}", self.location, self.device, params),
        }
    }
}

#[derive(Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Power {
    On,
    Off,
    HardOff,
    Error,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoboticaColorOut {
    pub hue: u16,
    pub saturation: u16,
    pub brightness: u16,
    pub kelvin: u16,
}

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

pub fn string_to_power(value: String) -> Power {
    match value.as_str() {
        "OFF" => Power::Off,
        "ON" => Power::On,
        "HARD_OFF" => Power::HardOff,
        _ => Power::Error,
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct MessageText {
    text: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct AudioMessage {
    message: MessageText,
}

pub fn string_to_message(str: String, topic: &str) -> Message {
    let msg = AudioMessage {
        message: MessageText { text: str },
    };
    let payload = serde_json::to_string(&msg).unwrap();
    Message::new(topic, payload, 0)
}
