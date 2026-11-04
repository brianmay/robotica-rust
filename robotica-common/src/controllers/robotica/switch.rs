//! A robotica switch controller
use crate::{
    mqtt::{topics::Topics, Json, MqttMessage},
    robotica::{
        commands::Command,
        switch::{DeviceAction, DeviceCommand, DevicePower},
    },
};
use serde::{Deserialize, Serialize};

use super::super::{
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
    /// The topic that reports the power state.
    #[must_use]
    pub fn power_topic(&self) -> String {
        format!("robotica/state/{}/power", self.topic_substr)
    }

    /// The topic that commands the switch.
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
            power: None,
        }
    }

    fn topics(&self) -> Topics {
        let mut topics = Topics::default();
        topics.subscribe_exact(self.power_topic());
        topics.send_exact(self.command_topic());
        topics
    }
}

/// A typed message from a robotica switch
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Message {
    /// The power state
    Power(DevicePower),
}

fn parse_power(msg: MqttMessage) -> Result<Message, ParseError> {
    let power: DevicePower = msg.try_into()?;
    Ok(Message::Power(power))
}

/// The controller for a switch
pub struct Controller {
    config: Config,
    power: Option<DevicePower>,
}

impl ControllerTrait for Controller {
    type Message = Message;

    fn subscriptions(&self) -> Vec<Subscription<Message>> {
        vec![Subscription {
            topic: self.config.power_topic(),
            parse: parse_power,
        }]
    }

    fn process_message(&mut self, message: Message) {
        match message {
            Message::Power(power) => {
                self.power = Some(power);
            }
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
            TurnOnOff::TurnOn => DeviceAction::TurnOn,
            TurnOnOff::TurnOff => DeviceAction::TurnOff,
        };

        let topic = self.config.command_topic();
        let command = DeviceCommand { action };
        let payload = Json(Command::Device(command));
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
            topic_substr: "kitchen/light".to_string(),
            action: Action::Toggle,
        }
    }

    #[test]
    fn test_topics() {
        let topics = config().topics();
        assert!(topics.is_subscribe_allowed("robotica/state/kitchen/light/power"));
        assert!(topics.is_send_allowed("robotica/command/kitchen/light"));
        assert!(!topics.is_subscribe_allowed("robotica/state/kitchen/light/scene"));
    }

    #[test]
    fn test_process_message() {
        let mut controller = config().create_controller();
        assert_eq!(controller.get_display_state(), DisplayState::Unknown);

        controller.process_message(Message::Power(DevicePower::On));
        assert_eq!(controller.get_display_state(), DisplayState::On);

        controller.process_message(Message::Power(DevicePower::Off));
        assert_eq!(controller.get_display_state(), DisplayState::Off);

        controller.process_message(Message::Power(DevicePower::HardOff));
        assert_eq!(controller.get_display_state(), DisplayState::HardOff);
    }
}
