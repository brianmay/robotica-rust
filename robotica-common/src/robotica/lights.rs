//! Stuff for robotica lights

use serde::{Deserialize, Serialize};

use crate::mqtt::MqttMessage;

/// A LIFX device's power level.
#[derive(Serialize, Deserialize)]
pub enum PowerLevel {
    /// The device is on.
    On,

    /// The device is off.
    Off,
}

/// A LIFX device's color.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HSBK {
    /// Hue, in degrees.
    pub hue: f32,

    /// Saturation, in percent.
    pub saturation: f32,

    /// Brightness, in percent.
    pub brightness: f32,

    /// Kelvin, in degrees.
    pub kelvin: u16,
}

impl Eq for HSBK {}

/// One or more colors
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Colors {
    /// A single color.
    Single(HSBK),

    /// A sequence of colors.
    Sequence(Vec<HSBK>),
}

/// A LIFX device's power level and color
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "power")]
#[serde(rename_all = "snake_case")]
pub enum PowerColor {
    /// The device is off.
    Off,

    /// The device is on and showing the specified colors
    On(Colors),
}

impl TryFrom<MqttMessage> for PowerColor {
    type Error = serde_json::Error;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        serde_json::from_str(&msg.payload)
    }
}

/// A V2 light power state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Power {
    /// The light is on
    On,

    /// The light is off
    Off,
}

/// Is the device online? What is its power level and color?
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum State {
    /// The device is online.
    Online(PowerColor),

    /// The device is offline.
    Offline,
}
