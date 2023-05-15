//! A robotica switch controller
use crate::{mqtt::MqttMessage, robotica::switch::DevicePower};
use serde::Deserialize;
use tracing::error;

use super::super::{
    get_display_state_for_action, get_press_on_or_off, json_command_vec, Action, ConfigTrait,
    ControllerTrait, DisplayState, Label, Subscription, TurnOnOff,
};

/// The configuration for a switch controller
#[derive(Clone, Deserialize)]
pub struct Config {
    /// The topic substring for the switch
    pub topic_substr: String,

    /// The action to take when the switch is clicked
    pub action: Action,
}

impl ConfigTrait for Config {
    type Controller = Controller;

    fn create_controller(&self) -> Controller {
        Controller {
            config: self.clone(),
            power: None,
        }
    }
}

/// The controller for a switch
pub struct Controller {
    config: Config,
    power: Option<DevicePower>,
}

fn topic(parts: &[&str]) -> String {
    parts.join("/")
}

impl ControllerTrait for Controller {
    fn get_subscriptions(&self) -> Vec<Subscription> {
        let mut result: Vec<Subscription> = Vec::new();
        let config = &self.config;

        let p = ["state", &config.topic_substr, "power"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::Power as u32,
        };
        result.push(s);

        result
    }

    fn process_message(&mut self, label: Label, data: MqttMessage) {
        if matches!(label.try_into(), Ok(ButtonStateMsgType::Power)) {
            match data.try_into() {
                Ok(json) => self.power = Some(json),
                Err(err) => error!("Invalid power state: {err}"),
            }
        } else {
            error!("Invalid message label {}", label);
        }
    }

    fn process_disconnected(&mut self) {
        self.power = None;
    }

    fn get_display_state(&self) -> DisplayState {
        let power = self.power;

        let state = match power {
            None => DisplayState::Unknown,
            Some(DevicePower::HardOff) => DisplayState::HardOff,
            Some(DevicePower::AutoOff) => DisplayState::AutoOff,
            Some(DevicePower::On) => DisplayState::On,
            Some(DevicePower::Off) => DisplayState::Off,
            _ => DisplayState::Error,
        };

        let action = self.config.action;
        get_display_state_for_action(state, action)
    }

    fn get_press_commands(&self) -> Vec<MqttMessage> {
        let display_state = self.get_display_state();
        let action = match get_press_on_or_off(display_state, self.config.action) {
            TurnOnOff::TurnOn => "turn_on",
            TurnOnOff::TurnOff => "turn_off",
        };

        let topic = format!("command/{}", self.config.topic_substr);
        let payload = serde_json::json!({ "action": action, "type" : "device" });
        json_command_vec(&topic, &payload)
    }

    fn get_action(&self) -> Action {
        self.config.action
    }
}

enum ButtonStateMsgType {
    Power,
}

impl TryFrom<u32> for ButtonStateMsgType {
    type Error = ();

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            x if x == ButtonStateMsgType::Power as u32 => Ok(ButtonStateMsgType::Power),
            _ => Err(()),
        }
    }
}
