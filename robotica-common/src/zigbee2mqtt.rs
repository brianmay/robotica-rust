//! Structures and functions for `Zigbee2MQTT` devices.
use std::fmt::Display;

use serde::Deserialize;

use crate::mqtt::MqttMessage;

/// The `Zigbee2MQTT` door.
#[allow(dead_code)]
#[derive(Deserialize)]
pub struct Door {
    battery: u8,
    battery_low: bool,
    contact: bool,
    #[serde(rename = "linkquality")]
    link_quality: u8,
    tamper: bool,
    voltage: u16,
}

impl Display for Door {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.contact {
            write!(f, "Closed")
        } else {
            write!(f, "Open")
        }
    }
}

impl TryFrom<MqttMessage> for Door {
    type Error = serde_json::Error;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        serde_json::from_str(&msg.payload)
    }
}
