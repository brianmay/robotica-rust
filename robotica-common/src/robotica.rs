//! Messages for Robotica

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::mqtt::MqttMessage;

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
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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

    /// UTF-8 error in power state.
    #[error("Invalid UTF8")]
    Utf8Error(#[from] std::str::Utf8Error),
}

impl TryFrom<MqttMessage> for DevicePower {
    type Error = PowerError;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        match msg.payload.as_str() {
            "ON" => Ok(DevicePower::On),
            "AUTO_OFF" => Ok(DevicePower::AutoOff),
            "OFF" => Ok(DevicePower::Off),
            "HARD_OFF" => Ok(DevicePower::HardOff),
            "ERROR" => Ok(DevicePower::DeviceError),
            _ => Err(PowerError::InvalidState(msg.payload)),
        }
    }
}

/// An audio command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCommand {
    /// The title of the message.
    pub title: String,

    /// The message to send.
    pub message: String,
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

    /// HDMI Command
    Hdmi(HdmiCommand),
}

impl TryFrom<MqttMessage> for Command {
    type Error = serde_json::Error;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        serde_json::from_str(&msg.payload)
    }
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
