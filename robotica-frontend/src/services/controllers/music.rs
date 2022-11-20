//! A robotica music controller
use log::error;
use robotica_common::mqtt::MqttMessage;

use super::{
    get_display_state_for_action, get_press_on_or_off, json_command, Action, ConfigTrait,
    ControllerTrait, DisplayState, Label, Subscription, TurnOnOff,
};

/// The configuration for a music controller
#[derive(Clone)]
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
            play_list: None,
        }
    }
}

/// The controller for a music
pub struct Controller {
    config: Config,
    play_list: Option<String>,
}

impl Controller {
    /// Create a new music controller
    #[must_use]
    pub fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
            play_list: None,
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

        let p = ["state", &config.topic_substr, "play_list"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::PlayList as u32,
        };
        result.push(s);

        result
    }

    fn process_message(&mut self, label: Label, data: String) {
        if let Ok(ButtonStateMsgType::PlayList) = label.try_into() {
            self.play_list = Some(data);
        } else {
            error!("Invalid message label {}", label);
        }
    }

    fn process_disconnected(&mut self) {
        self.play_list = None;
    }

    fn get_display_state(&self) -> DisplayState {
        let play_list = self.play_list.as_deref();
        let state = match play_list {
            None => DisplayState::Unknown,
            Some("ERROR") => DisplayState::Error,
            Some("STOP") => DisplayState::Off,
            Some(pl) if pl == self.config.play_list => DisplayState::On,
            _ => DisplayState::Off,
        };

        let action = self.config.action;
        get_display_state_for_action(state, action)
    }

    fn get_press_commands(&self) -> Vec<MqttMessage> {
        let display_state = self.get_display_state();
        let payload = match get_press_on_or_off(display_state, self.config.action) {
            TurnOnOff::TurnOn => {
                serde_json::json!({
                    "music": {"play_list": self.config.play_list}
                })
            }
            TurnOnOff::TurnOff => {
                serde_json::json!({
                    "music": {"stop": true}
                })
            }
        };

        let topic = format!("command/{}", self.config.topic_substr);
        json_command(&topic, &payload).map_or_else(Vec::new, |command| vec![command])
    }

    fn get_action(&self) -> Action {
        self.config.action
    }
}

enum ButtonStateMsgType {
    PlayList,
}

impl TryFrom<u32> for ButtonStateMsgType {
    type Error = ();

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            x if x == ButtonStateMsgType::PlayList as u32 => Ok(ButtonStateMsgType::PlayList),
            _ => Err(()),
        }
    }
}
