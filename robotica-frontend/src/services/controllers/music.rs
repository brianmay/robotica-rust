//! A robotica music controller
use log::error;

use super::{
    get_display_state_for_action, Action, Command, ConfigTrait, ControllerTrait, DisplayState,
    Label, Subscription,
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
            _ => DisplayState::OnOther,
        };

        let action = &self.config.action;
        get_display_state_for_action(state, action)
    }

    fn get_press_commands(&self) -> Vec<Command> {
        let play = match self.config.action {
            Action::TurnOn => true,
            Action::TurnOff => false,
            Action::Toggle => {
                let display_state = self.get_display_state();
                !matches!(display_state, DisplayState::On)
            }
        };

        let payload = if play {
            serde_json::json!({
                "music": {"play_list": self.config.play_list}
            })
        } else {
            serde_json::json!({
                "music": {"stop": true}
            })
        };

        let topic = format!("command/{}", self.config.topic_substr);
        let command = Command { topic, payload };

        vec![command]
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
