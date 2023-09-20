//! A zwave switch controller
use crate::{
    mqtt::{Json, MqttMessage},
    zwave::{Data, Status, StatusMessage},
};
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
}

impl ConfigTrait for Config {
    type Controller = Controller;

    fn create_controller(&self) -> Controller {
        Controller {
            config: self.clone(),
            status: None,
            power: None,
        }
    }
}

/// The controller for a switch
pub struct Controller {
    config: Config,
    status: Option<Status>,
    power: Option<bool>,
}

fn topic(parts: &[&str]) -> String {
    parts.join("/")
}

impl ControllerTrait for Controller {
    fn get_subscriptions(&self) -> Vec<Subscription> {
        let mut result: Vec<Subscription> = Vec::new();
        let config = &self.config;

        let p = ["zwave", &config.topic_substr, "status"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::Status as u32,
        };
        result.push(s);

        let p = ["zwave", &config.topic_substr, "37", "0", "targetValue"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::Power as u32,
        };
        result.push(s);

        result
    }

    fn process_message(&mut self, label: Label, data: MqttMessage) {
        match label.try_into() {
            Ok(ButtonStateMsgType::Status) => {
                let maybe_msg: Result<Json<StatusMessage>, _> = data.try_into();
                match maybe_msg {
                    Ok(Json(msg)) => self.status = Some(msg.status),
                    Err(err) => error!("Invalid power state: {err}"),
                }
            }

            Ok(ButtonStateMsgType::Power) => {
                let maybe_msg: Result<Json<Data<bool>>, _> = data.try_into();
                match maybe_msg {
                    Ok(Json(json)) => self.power = Some(json.value),
                    Err(err) => error!("Invalid power state: {err}"),
                }
            }

            Err(_) => error!("Invalid message label {}", label),
        }
    }

    fn process_disconnected(&mut self) {
        self.power = None;
    }

    fn get_display_state(&self) -> DisplayState {
        let state = match (self.status, self.power) {
            (Some(Status::Dead), _) => DisplayState::Error,
            (Some(Status::Alive), None) => DisplayState::Unknown,
            (Some(Status::Alive), Some(false)) => DisplayState::Off,
            (Some(Status::Alive), Some(true)) => DisplayState::On,
            (None, _) => return DisplayState::Unknown,
        };

        let action = self.config.action;
        get_display_state_for_action(state, action)
    }

    fn get_press_commands(&self) -> Vec<MqttMessage> {
        let display_state = self.get_display_state();
        let payload = match get_press_on_or_off(display_state, self.config.action) {
            TurnOnOff::TurnOn => "True",
            TurnOnOff::TurnOff => "False",
        };

        let topic = format!("zwave/{}/37/0/targetValue/set", self.config.topic_substr);
        mqtt_command_vec(&topic, &payload.to_string())
    }

    fn get_action(&self) -> Action {
        self.config.action
    }
}

enum ButtonStateMsgType {
    Status,
    Power,
}

impl TryFrom<u32> for ButtonStateMsgType {
    type Error = ();

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            x if x == ButtonStateMsgType::Status as u32 => Ok(ButtonStateMsgType::Status),
            x if x == ButtonStateMsgType::Power as u32 => Ok(ButtonStateMsgType::Power),
            _ => Err(()),
        }
    }
}
