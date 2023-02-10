//! A robotica HDMI controller
use crate::mqtt::MqttMessage;

use super::{
    get_display_state_for_action, get_press_on_or_off, json_command_vec, Action, ConfigTrait,
    ControllerTrait, DisplayState, Label, Subscription, TurnOnOff,
};

/// The configuration for a switch controller
#[derive(Clone)]
pub struct Config {
    /// The topic substring for the switch
    pub topic_substr: String,

    /// The action to take when the switch is clicked
    pub action: Action,

    /// The input of the switch
    pub input: u8,

    /// The output of the switch
    pub output: u8,
}

impl ConfigTrait for Config {
    type Controller = Controller;

    fn create_controller(&self) -> Controller {
        Controller {
            config: self.clone(),
            device_state: DeviceState::Unknown,
        }
    }
}

enum DeviceState {
    SelectedInput(u8),
    HardOff,
    Error,
    Unknown,
}

/// The controller for a switch
pub struct Controller {
    config: Config,
    device_state: DeviceState,
}

fn topic(parts: &[&str]) -> String {
    parts.join("/")
}

impl ControllerTrait for Controller {
    fn get_subscriptions(&self) -> Vec<Subscription> {
        let mut result: Vec<Subscription> = Vec::new();
        let config = &self.config;

        let p = [
            "state",
            &config.topic_substr,
            &format!("output{}", config.output),
        ];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::Output as u32,
        };
        result.push(s);

        result
    }

    fn process_message(&mut self, _label: Label, data: String) {
        match (data.parse(), data.as_ref()) {
            (Ok(value), _) => {
                self.device_state = DeviceState::SelectedInput(value);
            }
            (_, "HARD_OFF") => {
                self.device_state = DeviceState::HardOff;
            }
            (_, _) => {
                self.device_state = DeviceState::Error;
            }
        }
    }

    fn process_disconnected(&mut self) {
        self.device_state = DeviceState::Unknown;
    }

    fn get_display_state(&self) -> DisplayState {
        let device_state = &self.device_state;
        let input = &self.config.input;

        let state = match device_state {
            DeviceState::Unknown => DisplayState::Unknown,
            DeviceState::HardOff => DisplayState::HardOff,
            DeviceState::SelectedInput(i) if i == input => DisplayState::On,
            DeviceState::SelectedInput(_) => DisplayState::Off,
            DeviceState::Error => DisplayState::Error,
        };

        let action = self.config.action;
        get_display_state_for_action(state, action)
    }

    fn get_press_commands(&self) -> Vec<MqttMessage> {
        let payload = serde_json::json!({
            "input": self.config.input,
            "output": self.config.output,
            "type": "hdmi",
        });

        let display_state = self.get_display_state();
        if matches!(
            get_press_on_or_off(display_state, self.config.action),
            TurnOnOff::TurnOn
        ) {
            let topic = format!("command/{}", self.config.topic_substr);
            json_command_vec(&topic, &payload)
        } else {
            // Not possible to turn off an input, so do nothing.
            vec![]
        }
    }

    fn get_action(&self) -> Action {
        self.config.action
    }
}

enum ButtonStateMsgType {
    Output,
}

impl TryFrom<u32> for ButtonStateMsgType {
    type Error = ();

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            x if x == ButtonStateMsgType::Output as u32 => Ok(ButtonStateMsgType::Output),
            _ => Err(()),
        }
    }
}
