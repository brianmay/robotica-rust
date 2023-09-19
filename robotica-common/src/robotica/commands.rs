//! Commands for Robotica

use serde::{Deserialize, Serialize};

use super::{audio::AudioCommand, hdmi::HdmiCommand, lights::LightCommand, switch::DeviceCommand};

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

    // FIXME: Remove
    /// Light Command (deprecated)
    Light2(LightCommand),

    /// HDMI Command
    Hdmi(HdmiCommand),
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use crate::robotica::switch::{DeviceAction, DevicePower};

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
