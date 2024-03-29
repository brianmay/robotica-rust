//! A Tasmota switch controller
use crate::mqtt::MqttMessage;
use serde::{Deserialize, Serialize};
use tracing::error;

use super::{
    get_display_state_for_action, get_press_on_or_off, mqtt_command_vec, Action, ConfigTrait,
    ControllerTrait, DisplayState, Label, Subscription, TurnOnOff,
};

/// The configuration for a switch controller
#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
pub struct Config {
    /// The topic substring for the switch
    pub topic_substr: String,

    /// The action to take when the switch is clicked
    pub action: Action,

    /// The postfix for the power topic
    pub power_postfix: String,
}

impl ConfigTrait for Config {
    type Controller = Controller;

    fn create_controller(&self) -> Controller {
        Controller {
            config: self.clone(),
            power: None,
            online: false,
        }
    }
}

/// The controller for a switch
pub struct Controller {
    config: Config,
    power: Option<String>,
    online: bool,
}

impl ControllerTrait for Controller {
    fn get_subscriptions(&self) -> Vec<Subscription> {
        let mut result: Vec<Subscription> = Vec::new();
        let config = &self.config;

        let topic = format!("stat/{}/POWER{}", config.topic_substr, config.power_postfix);
        let s = Subscription {
            topic,
            label: ButtonStateMsgType::Power as u32,
        };
        result.push(s);

        let topic = format!("tele/{}/LWT", config.topic_substr);
        let s = Subscription {
            topic,
            label: ButtonStateMsgType::Lwt as u32,
        };
        result.push(s);

        result
    }

    fn process_message(&mut self, label: Label, data: MqttMessage) {
        if matches!(label.try_into(), Ok(ButtonStateMsgType::Power)) {
            match data.try_into() {
                Ok(s) => self.power = Some(s),
                Err(e) => error!("Invalid message data {}", e),
            }
            self.online = true;
        } else if matches!(label.try_into(), Ok(ButtonStateMsgType::Lwt)) {
            match data.payload_as_str() {
                Ok("Online") => self.online = true,
                _ => self.online = false,
            }
        } else {
            error!("Invalid message label {}", label);
        }
    }

    fn process_disconnected(&mut self) {
        self.power = None;
        self.online = false;
    }

    fn get_display_state(&self) -> DisplayState {
        let power = self.power.as_deref();

        let state = match (power, self.online) {
            (None, _) => DisplayState::Unknown,
            (_, false) => DisplayState::HardOff,
            (Some("ON"), true) => DisplayState::On,
            (Some("OFF"), true) => DisplayState::Off,
            _ => DisplayState::Error,
        };

        let action = self.config.action;
        get_display_state_for_action(state, action)
    }

    fn get_press_commands(&self) -> Vec<MqttMessage> {
        let config = &self.config;

        let display_state = self.get_display_state();
        let payload = match get_press_on_or_off(display_state, self.config.action) {
            TurnOnOff::TurnOn => "ON",
            TurnOnOff::TurnOff => "OFF",
        };

        let topic = format!("cmnd/{}/POWER{}", config.topic_substr, config.power_postfix);
        mqtt_command_vec(&topic, &payload.to_string())
    }

    fn get_action(&self) -> Action {
        self.config.action
    }
}

enum ButtonStateMsgType {
    Power,
    Lwt,
}

impl TryFrom<u32> for ButtonStateMsgType {
    type Error = ();

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            x if x == ButtonStateMsgType::Power as u32 => Ok(ButtonStateMsgType::Power),
            x if x == ButtonStateMsgType::Lwt as u32 => Ok(ButtonStateMsgType::Lwt),
            _ => Err(()),
        }
    }
}
