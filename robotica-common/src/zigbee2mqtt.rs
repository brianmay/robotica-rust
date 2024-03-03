//! Structures and functions for `Zigbee2MQTT` devices.
use std::fmt::Display;

use serde::Deserialize;

use crate::mqtt::Json;

/// The `Zigbee2MQTT` door.
#[allow(dead_code)]
#[derive(Deserialize, Clone, Eq, PartialEq)]
pub struct Door {
    // We don't care about battery status just yet.
    // battery: f32,
    // battery_low: bool,
    contact: bool,
    #[serde(rename = "linkquality")]
    link_quality: u8,
    tamper: bool,
    voltage: u16,
}

#[derive(Clone, Eq, PartialEq, Debug)]
/// The state of a `Zigbee2MQTT` door.
pub enum DoorState {
    /// The door is open.
    Open,

    /// The door is closed.
    Closed,
}

impl Display for DoorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DoorState::Open => write!(f, "Open"),
            DoorState::Closed => write!(f, "Closed"),
        }
    }
}

impl From<Door> for DoorState {
    fn from(door: Door) -> Self {
        if door.contact {
            DoorState::Closed
        } else {
            DoorState::Open
        }
    }
}

impl From<Json<Door>> for DoorState {
    fn from(door: Json<Door>) -> Self {
        door.0.into()
    }
}
