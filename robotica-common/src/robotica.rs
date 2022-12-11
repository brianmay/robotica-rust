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
#[derive(Debug, Clone, Eq, PartialEq)]
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
    /// The message to send.
    pub message: String,
}

/// A command to send to any device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum Command {
    /// A command to send to a switch.
    Device(DeviceCommand),

    /// Dummy value
    Audio(AudioCommand),

    /// Dummy value
    Dummy2,
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
}
