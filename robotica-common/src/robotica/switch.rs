//! Messages for Robotica switches
use std::{
    fmt::{Display, Formatter},
    str::Utf8Error,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::mqtt::MqttMessage;

/// An action to send to a switch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceAction {
    /// Turn the switch on.
    TurnOn,

    /// Turn the switch off.
    TurnOff,
}

/// A command to send to a switch
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceCommand {
    /// The action to perform.
    pub action: DeviceAction,
}

impl Display for DeviceCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let action = &self.action;
        write!(f, "Device {action:?}")
    }
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

impl DevicePower {
    /// Returns true if the device is on.
    #[must_use]
    pub const fn is_on(&self) -> bool {
        matches!(self, DevicePower::On)
    }
}

impl From<DevicePower> for String {
    /// Convert the power state to a string.
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
