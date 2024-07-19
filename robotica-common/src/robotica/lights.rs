//! Messages for robotica lights

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

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
#[serde(rename_all = "snake_case")]
pub enum State {
    /// The device is online.
    Online(PowerColor),

    /// The device is offline.
    Offline,
}

/// Is the device offline, on, or off?
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerState {
    /// The device is offline.
    Offline,

    /// The device is on.
    On,

    /// The device is off.
    Off,
}

/// A color for a light.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoboticaColor {
    /// The hue of the color, 0-359.
    pub hue: u16,

    /// The saturation of the color, 0-100.
    pub saturation: u16,

    /// The brightness of the color, 0-100.
    pub brightness: u16,

    /// The kelvin of the color, 2500-9000.
    pub kelvin: u16,
}

/// A V2 command to send to a light
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "action")]
pub enum LightCommand {
    /// Turn the switch on.
    TurnOn {
        /// The scene to use
        scene: String,
    },

    /// Turn the switch off.
    TurnOff,

    /// Flash the light.
    Flash,
}

impl Display for LightCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LightCommand::TurnOn { scene } => {
                write!(f, "turn_on scene {scene}")
            }
            LightCommand::TurnOff => write!(f, "turn_off"),
            LightCommand::Flash => {
                write!(f, "flash")
            }
        }
    }
}
