//! A zwave switch controller
use crate::{
    mqtt::{topics::Topics, Json, MqttMessage},
    zwave::{Data, Status, StatusMessage},
};
use serde::{Deserialize, Serialize};

use super::{
    get_display_state_for_action, get_press_on_or_off, mqtt_command_vec, Action, ConfigTrait,
    ControllerTrait, DisplayState, ParseError, Subscription, TurnOnOff,
};

/// The configuration for a switch controller
#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
pub struct Config {
    /// The topic substring for the switch
    pub topic_substr: String,

    /// The action to take when the switch is clicked
    pub action: Action,
}

impl Config {
    /// The topic that reports the device status.
    #[must_use]
    pub fn status_topic(&self) -> String {
        format!("zwave/{}/status", self.topic_substr)
    }

    /// The topic that reports the target power value.
    #[must_use]
    pub fn power_topic(&self) -> String {
        format!("zwave/{}/37/0/targetValue", self.topic_substr)
    }

    /// The topic that commands the target power value.
    #[must_use]
    pub fn command_topic(&self) -> String {
        format!("zwave/{}/37/0/targetValue/set", self.topic_substr)
    }
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

    fn topics(&self) -> Topics {
        let mut topics = Topics::default();
        topics.subscribe_exact(self.status_topic());
        topics.subscribe_exact(self.power_topic());
        topics.send_exact(self.command_topic());
        topics
    }
}

/// A typed message from a zwave device
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Message {
    /// The device status
    Status(Status),

    /// The target power value
    Power(bool),
}

fn parse_status(msg: MqttMessage) -> Result<Message, ParseError> {
    let Json(message): Json<StatusMessage> = msg.try_into()?;
    Ok(Message::Status(message.status))
}

fn parse_power(msg: MqttMessage) -> Result<Message, ParseError> {
    let Json(data): Json<Data<bool>> = msg.try_into()?;
    Ok(Message::Power(data.value))
}

/// The controller for a switch
pub struct Controller {
    config: Config,
    status: Option<Status>,
    power: Option<bool>,
}

impl ControllerTrait for Controller {
    type Message = Message;

    fn subscriptions(&self) -> Vec<Subscription<Message>> {
        vec![
            Subscription {
                topic: self.config.status_topic(),
                parse: parse_status,
            },
            Subscription {
                topic: self.config.power_topic(),
                parse: parse_power,
            },
        ]
    }

    fn process_message(&mut self, message: Message) {
        match message {
            Message::Status(status) => {
                self.status = Some(status);
            }
            Message::Power(power) => {
                self.power = Some(power);
            }
        }
    }

    fn process_disconnected(&mut self) {
        self.power = None;
    }

    fn get_display_state(&self) -> DisplayState {
        #[allow(clippy::match_same_arms)]
        let state = match (self.status, self.power) {
            (Some(Status::Dead), _) => DisplayState::Error,
            (Some(Status::Unknown), _) => DisplayState::Unknown,
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

        let topic = self.config.command_topic();
        mqtt_command_vec(&topic, &payload.to_string())
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
            topic_substr: "lounge_light".to_string(),
            action: Action::Toggle,
        }
    }

    #[test]
    fn test_topics() {
        let topics = config().topics();
        assert!(topics.is_subscribe_allowed("zwave/lounge_light/status"));
        assert!(topics.is_subscribe_allowed("zwave/lounge_light/37/0/targetValue"));
        assert!(topics.is_send_allowed("zwave/lounge_light/37/0/targetValue/set"));
        assert!(!topics.is_subscribe_allowed("zwave/other/status"));
    }

    #[test]
    fn test_process_message() {
        let mut controller = config().create_controller();

        controller.process_message(Message::Status(Status::Alive));
        assert_eq!(controller.get_display_state(), DisplayState::Unknown);

        controller.process_message(Message::Power(true));
        assert_eq!(controller.get_display_state(), DisplayState::On);

        controller.process_message(Message::Power(false));
        assert_eq!(controller.get_display_state(), DisplayState::Off);

        controller.process_message(Message::Status(Status::Dead));
        assert_eq!(controller.get_display_state(), DisplayState::Error);
    }
}
