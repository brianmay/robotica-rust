//! A Tasmota switch controller
use crate::mqtt::{topics::Topics, MqttMessage};
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

    /// The postfix for the power topic
    pub power_postfix: String,
}

impl Config {
    /// The topic that reports the power state.
    #[must_use]
    pub fn power_topic(&self) -> String {
        format!("stat/{}/POWER{}", self.topic_substr, self.power_postfix)
    }

    /// The topic that reports the last-will-and-testament state.
    #[must_use]
    pub fn lwt_topic(&self) -> String {
        format!("tele/{}/LWT", self.topic_substr)
    }

    /// The topic that commands the power state.
    #[must_use]
    pub fn command_topic(&self) -> String {
        format!("cmnd/{}/POWER{}", self.topic_substr, self.power_postfix)
    }
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

    fn topics(&self) -> Topics {
        let mut topics = Topics::default();
        topics.subscribe_exact(self.power_topic());
        topics.subscribe_exact(self.lwt_topic());
        topics.send_exact(self.command_topic());
        topics
    }
}

/// A typed message from a tasmota device
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Message {
    /// The power state payload (typically "ON" or "OFF")
    Power(String),

    /// The last-will-and-testament state; true when the device is online
    Lwt(bool),
}

fn parse_power(msg: MqttMessage) -> Result<Message, ParseError> {
    let power: String = msg.try_into()?;
    Ok(Message::Power(power))
}

// The signature must match `ParseFn`, which owns the message.
#[allow(clippy::needless_pass_by_value)]
fn parse_lwt(msg: MqttMessage) -> Result<Message, ParseError> {
    let online = matches!(msg.payload_as_str()?, "Online");
    Ok(Message::Lwt(online))
}

/// The controller for a switch
pub struct Controller {
    config: Config,
    power: Option<String>,
    online: bool,
}

impl ControllerTrait for Controller {
    type Message = Message;

    fn subscriptions(&self) -> Vec<Subscription<Message>> {
        vec![
            Subscription {
                topic: self.config.power_topic(),
                parse: parse_power,
            },
            Subscription {
                topic: self.config.lwt_topic(),
                parse: parse_lwt,
            },
        ]
    }

    fn process_message(&mut self, message: Message) {
        match message {
            Message::Power(power) => {
                self.power = Some(power);
                self.online = true;
            }
            Message::Lwt(online) => {
                self.online = online;
            }
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

        let topic = config.command_topic();
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
            topic_substr: "kitchen_fan".to_string(),
            action: Action::Toggle,
            power_postfix: "2".to_string(),
        }
    }

    #[test]
    fn test_topics() {
        let topics = config().topics();
        assert!(topics.is_subscribe_allowed("stat/kitchen_fan/POWER2"));
        assert!(topics.is_subscribe_allowed("tele/kitchen_fan/LWT"));
        assert!(topics.is_send_allowed("cmnd/kitchen_fan/POWER2"));
        assert!(!topics.is_subscribe_allowed("stat/kitchen_fan/POWER"));
        assert!(!topics.is_send_allowed("cmnd/kitchen_fan/POWER"));
    }

    #[test]
    fn test_process_message() {
        let mut controller = config().create_controller();
        assert_eq!(controller.get_display_state(), DisplayState::Unknown);

        controller.process_message(Message::Lwt(true));
        controller.process_message(Message::Power("ON".to_string()));
        assert_eq!(controller.get_display_state(), DisplayState::On);

        controller.process_message(Message::Power("OFF".to_string()));
        assert_eq!(controller.get_display_state(), DisplayState::Off);

        controller.process_message(Message::Lwt(false));
        assert_eq!(controller.get_display_state(), DisplayState::HardOff);
    }

    #[test]
    fn test_parse_power() {
        let msg = MqttMessage::new(
            "stat/foo/POWER",
            "ON",
            crate::mqtt::Retain::NoRetain,
            crate::mqtt::QoS::AtMostOnce,
        );
        assert_eq!(parse_power(msg).unwrap(), Message::Power("ON".to_string()));
    }

    #[test]
    fn test_parse_lwt() {
        let msg = MqttMessage::new(
            "tele/foo/LWT",
            "Online",
            crate::mqtt::Retain::NoRetain,
            crate::mqtt::QoS::AtMostOnce,
        );
        assert_eq!(parse_lwt(msg).unwrap(), Message::Lwt(true));

        let msg = MqttMessage::new(
            "tele/foo/LWT",
            "Offline",
            crate::mqtt::Retain::NoRetain,
            crate::mqtt::QoS::AtMostOnce,
        );
        assert_eq!(parse_lwt(msg).unwrap(), Message::Lwt(false));
    }
}
