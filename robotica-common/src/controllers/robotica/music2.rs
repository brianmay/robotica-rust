//! A robotica music controller
use crate::{
    mqtt::{Json, MqttMessage},
    robotica::audio,
};
use serde::Deserialize;
use tracing::error;

use super::super::{
    get_display_state_for_action, get_press_on_or_off, json_command_vec, Action, ConfigTrait,
    ControllerTrait, DisplayState, Label, Subscription, TurnOnOff,
};

/// The configuration for a music controller
#[derive(Clone, Deserialize)]
pub struct Config {
    /// The topic substring for the music
    pub topic_substr: String,

    /// The action to take when the music is clicked
    pub action: Action,

    /// The playlist to use for the music
    pub play_list: String,
}

impl ConfigTrait for Config {
    type Controller = Controller;

    fn create_controller(&self) -> Controller {
        Controller {
            config: self.clone(),
            state: None,
        }
    }
}

/// The controller for a music
pub struct Controller {
    config: Config,
    state: Option<audio::State>,
}

impl Controller {
    /// Create a new music controller
    #[must_use]
    pub fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
            state: None,
        }
    }
}

fn topic(parts: &[&str]) -> String {
    parts.join("/")
}

impl ControllerTrait for Controller {
    fn get_subscriptions(&self) -> Vec<Subscription> {
        let mut result: Vec<Subscription> = Vec::new();
        let config = &self.config;

        let p = ["state", &config.topic_substr];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::State as u32,
        };
        result.push(s);

        result
    }

    fn process_message(&mut self, label: Label, data: MqttMessage) {
        #[allow(clippy::single_match_else)]
        match label.try_into() {
            Ok(ButtonStateMsgType::State) => match data.try_into() {
                Ok(Json(state)) => self.state = Some(state),
                Err(e) => error!("Invalid state: {e}"),
            },

            Err(_) => error!("Invalid message label {}", label),
        }
    }

    fn process_disconnected(&mut self) {
        self.state = None;
    }

    fn get_display_state(&self) -> DisplayState {
        let state = match &self.state {
            None => DisplayState::Unknown,
            Some(audio::State {
                error: Some(..), ..
            }) => DisplayState::Error,

            Some(audio::State {
                play_list: Some(play_list),
                ..
            }) if *play_list == self.config.play_list => DisplayState::On,

            Some(audio::State { .. }) => DisplayState::Off,
        };

        let action = self.config.action;
        get_display_state_for_action(state, action)
    }

    fn get_press_commands(&self) -> Vec<MqttMessage> {
        let display_state = self.get_display_state();
        let payload = match get_press_on_or_off(display_state, self.config.action) {
            TurnOnOff::TurnOn => {
                serde_json::json!({
                    "music": {"play_list": self.config.play_list},
                    "type": "audio"
                })
            }
            TurnOnOff::TurnOff => {
                serde_json::json!({
                    "music": {"stop": true},
                    "type": "audio",
                })
            }
        };

        let topic = format!("command/{}", self.config.topic_substr);
        json_command_vec(&topic, &payload)
    }

    fn get_action(&self) -> Action {
        self.config.action
    }
}

enum ButtonStateMsgType {
    State,
}

impl TryFrom<u32> for ButtonStateMsgType {
    type Error = ();

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            x if x == ButtonStateMsgType::State as u32 => Ok(ButtonStateMsgType::State),
            _ => Err(()),
        }
    }
}
