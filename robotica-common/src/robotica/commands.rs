//! Commands for Robotica

use std::str::Utf8Error;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::mqtt::MqttMessage;

use super::tasks::SubTask;

/// An action to send to a switch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceAction {
    /// Turn the switch on.
    TurnOn,

    /// Turn the switch off.
    TurnOff,
}

/// A command to send to a switch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCommand {
    /// The action to perform.
    pub action: DeviceAction,
}

/// The status from a switch
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DevicePower {
    /// The switch is on.
    On,
    /// The switch is on, but automatically turned off.
    AutoOff,
    /// The switch is off.
    Off,
    /// The switch is turned off and cannot be turned on.
    HardOff,
    /// The switch is encountering an error.
    DeviceError,
}

impl From<DevicePower> for String {
    /// Convert the power state to a string.
    #[must_use]
    fn from(power: DevicePower) -> Self {
        match power {
            DevicePower::On => "ON".to_string(),
            DevicePower::AutoOff => "AUTO_OFF".to_string(),
            DevicePower::Off => "OFF".to_string(),
            DevicePower::HardOff => "HARD_OFF".to_string(),
            DevicePower::DeviceError => "ERROR".to_string(),
        }
    }
}

/// An error that can occur when parsing a switch power.
#[derive(Error, Debug)]
pub enum PowerError {
    /// The power state is invalid.
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// The payload was not a valid UTF-8 string.
    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] Utf8Error),
}

impl TryFrom<MqttMessage> for DevicePower {
    type Error = PowerError;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        let payload = msg.payload_as_str()?;

        match payload {
            "ON" => Ok(DevicePower::On),
            "AUTO_OFF" => Ok(DevicePower::AutoOff),
            "OFF" => Ok(DevicePower::Off),
            "HARD_OFF" => Ok(DevicePower::HardOff),
            "ERROR" => Ok(DevicePower::DeviceError),
            _ => Err(PowerError::InvalidState(payload.to_string())),
        }
    }
}

/// A command to play some music
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicCommand {
    /// The playlist to play.
    pub play_list: Option<String>,

    /// Should we stop playing music?
    pub stop: Option<bool>,
}

/// A command to change the volumes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeCommand {
    /// The music volume to set.
    pub music: Option<u8>,

    /// The message volume to set.
    pub message: Option<u8>,
}

/// An audio command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCommand {
    /// The title of the message.
    pub title: Option<String>,

    /// The message to send.
    pub message: Option<String>,

    /// The sounds to play.
    pub sound: Option<String>,

    /// The music to play.
    pub music: Option<MusicCommand>,

    /// The volume to set.
    pub volume: Option<VolumeCommand>,

    /// Pre tasks to execute before playing the message.
    pub pre_tasks: Option<Vec<SubTask>>,

    /// Post tasks to execute after playing the message.
    pub post_tasks: Option<Vec<SubTask>>,
}

impl AudioCommand {
    /// Create a new message only command
    #[must_use]
    pub fn new_message(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            message: Some(message.into()),
            sound: None,
            music: None,
            volume: None,
            pre_tasks: None,
            post_tasks: None,
        }
    }
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

/// A command to send to a light.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LightCommand {
    /// The action to perform.
    pub action: Option<DeviceAction>,

    /// The color to set.
    pub color: Option<RoboticaColor>,

    /// The scene to set.
    pub scene: Option<String>,
}

/// A V2 command to send to a light
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "action")]
pub enum Light2Command {
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

/// A command to send to a HDMI matrix.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HdmiCommand {
    /// The input to switch to.
    pub input: u8,

    /// The output to switch.
    pub output: u8,
}

/// A command to send to any device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum Command {
    /// A command to send to a switch.
    Device(DeviceCommand),

    /// Audio command
    Audio(AudioCommand),

    /// Light Command
    Light(LightCommand),

    /// Light V2 Command
    Light2(Light2Command),

    /// HDMI Command
    Hdmi(HdmiCommand),
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use serde_json::json;

    #[test]
    fn test_switch_command() {
        let command = DeviceCommand {
            action: DeviceAction::TurnOn,
        };
        let json = json!({
            "action": "turn_on",
        });
        assert_eq!(json, serde_json::to_value(command).unwrap());
    }

    #[test]
    fn test_command() {
        let command = Command::Device(DeviceCommand {
            action: DeviceAction::TurnOn,
        });
        let json = json!({
            "type": "device",
            "action": "turn_on",
        });
        assert_eq!(json, serde_json::to_value(command).unwrap());
    }

    #[test]
    fn test_device_power() {
        assert_eq!(
            "HARD_OFF",
            serde_json::to_value(DevicePower::HardOff).unwrap()
        );

        assert_eq!(
            DevicePower::HardOff,
            serde_json::from_value(json!("HARD_OFF")).unwrap()
        );

        let str: String = DevicePower::HardOff.into();
        assert_eq!("HARD_OFF".to_string(), str);
    }
}
