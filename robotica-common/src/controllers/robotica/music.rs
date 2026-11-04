//! A robotica music controller
use crate::{
    mqtt::{topics::Topics, Json, MqttMessage},
    robotica::{
        audio::{self, AudioCommand, MusicCommand},
        commands::Command,
    },
};
use serde::{Deserialize, Serialize};

use super::super::{
    get_display_state_for_action, get_press_on_or_off, mqtt_command_vec, Action, ConfigTrait,
    ControllerTrait, DisplayState, ParseError, Subscription, TurnOnOff,
};

/// The configuration for a music controller
#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
pub struct Config {
    /// The topic substring for the music
    pub topic_substr: String,

    /// The action to take when the music is clicked
    pub action: Action,

    /// The playlist to use for the music
    pub play_list: String,
}

impl Config {
    /// The topic that reports the audio status.
    #[must_use]
    pub fn status_topic(&self) -> String {
        format!("robotica/state/{}/status", self.topic_substr)
    }

    /// The topic that commands the audio player.
    #[must_use]
    pub fn command_topic(&self) -> String {
        format!("robotica/command/{}", self.topic_substr)
    }
}

impl ConfigTrait for Config {
    type Controller = Controller;

    fn create_controller(&self) -> Controller {
        Controller {
            config: self.clone(),
            state: None,
        }
    }

    fn topics(&self) -> Topics {
        let mut topics = Topics::default();
        topics.subscribe_exact(self.status_topic());
        topics.send_exact(self.command_topic());
        topics
    }
}

/// A typed message from the audio player
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Message {
    /// The audio player status
    Status(audio::State),
}

fn parse_status(msg: MqttMessage) -> Result<Message, ParseError> {
    let Json(state): Json<audio::State> = msg.try_into()?;
    Ok(Message::Status(state))
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

impl ControllerTrait for Controller {
    type Message = Message;

    fn subscriptions(&self) -> Vec<Subscription<Message>> {
        vec![Subscription {
            topic: self.config.status_topic(),
            parse: parse_status,
        }]
    }

    fn process_message(&mut self, message: Message) {
        match message {
            Message::Status(state) => {
                self.state = Some(state);
            }
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
        let command = match get_press_on_or_off(display_state, self.config.action) {
            TurnOnOff::TurnOn => MusicCommand {
                play_list: Some(self.config.play_list.clone()),
                stop: None,
            },
            TurnOnOff::TurnOff => MusicCommand {
                play_list: None,
                stop: Some(true),
            },
        };

        let audio_command = AudioCommand {
            pre_tasks: None,
            post_tasks: None,
            message: None,
            music: Some(command),
            volume: None,
        };
        let payload = Json(Command::Audio(audio_command));
        let topic = self.config.command_topic();
        mqtt_command_vec(&topic, &payload)
    }

    fn get_action(&self) -> Action {
        self.config.action
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn config() -> Config {
        Config {
            topic_substr: "lounge/music".to_string(),
            action: Action::Toggle,
            play_list: "relax".to_string(),
        }
    }

    #[test]
    fn test_topics() {
        let topics = config().topics();
        assert!(topics.is_subscribe_allowed("robotica/state/lounge/music/status"));
        assert!(topics.is_send_allowed("robotica/command/lounge/music"));
        assert!(!topics.is_subscribe_allowed("robotica/state/lounge/music/power"));
    }

    #[test]
    fn test_process_message() {
        let mut controller = config().create_controller();
        assert_eq!(controller.get_display_state(), DisplayState::Unknown);

        controller.process_message(Message::Status(audio::State {
            play_list: Some("relax".to_string()),
            ..audio::State::default()
        }));
        assert_eq!(controller.get_display_state(), DisplayState::On);

        controller.process_message(Message::Status(audio::State::default()));
        assert_eq!(controller.get_display_state(), DisplayState::Off);
    }
}
