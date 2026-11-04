//! A robotica HDMI controller
use serde::{Deserialize, Serialize};

use crate::{
    mqtt::{topics::Topics, Json, MqttMessage},
    robotica::{commands::Command, hdmi_matrix::HdmiMatrixCommand},
};

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

    /// The input of the switch
    pub input: u8,

    /// The output of the switch
    pub output: u8,
}

impl Config {
    /// The topic that reports the selected input for the output.
    #[must_use]
    pub fn output_topic(&self) -> String {
        format!("robotica/state/{}/output{}", self.topic_substr, self.output)
    }

    /// The topic that commands the matrix.
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
            device_state: DeviceState::Unknown,
        }
    }

    fn topics(&self) -> Topics {
        let mut topics = Topics::default();
        topics.subscribe_exact(self.output_topic());
        topics.send_exact(self.command_topic());
        topics
    }
}

/// A typed message from an HDMI matrix output
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Message {
    /// The output has the given input selected
    SelectedInput(u8),

    /// The device is off
    HardOff,

    /// The value could not be parsed
    Invalid,
}

// The signature must match `ParseFn`, which owns the message.
#[allow(clippy::needless_pass_by_value)]
fn parse_output(msg: MqttMessage) -> Result<Message, ParseError> {
    let body = msg.payload_as_str()?;
    let message = match body.parse::<u8>() {
        Ok(value) => Message::SelectedInput(value),
        Err(_) if body == "HARD_OFF" => Message::HardOff,
        Err(_) => Message::Invalid,
    };
    Ok(message)
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

impl ControllerTrait for Controller {
    type Message = Message;

    fn subscriptions(&self) -> Vec<Subscription<Message>> {
        vec![Subscription {
            topic: self.config.output_topic(),
            parse: parse_output,
        }]
    }

    fn process_message(&mut self, message: Message) {
        self.device_state = match message {
            Message::SelectedInput(value) => DeviceState::SelectedInput(value),
            Message::HardOff => DeviceState::HardOff,
            Message::Invalid => DeviceState::Error,
        };
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
        let payload = Json(Command::HdmiMatrix(HdmiMatrixCommand {
            input: self.config.input,
            output: self.config.output,
        }));

        let display_state = self.get_display_state();
        if matches!(
            get_press_on_or_off(display_state, self.config.action),
            TurnOnOff::TurnOn
        ) {
            let topic = self.config.command_topic();
            mqtt_command_vec(&topic, &payload)
        } else {
            // Not possible to turn off an input, so do nothing.
            vec![]
        }
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
            topic_substr: "lounge/matrix".to_string(),
            action: Action::TurnOn,
            input: 3,
            output: 2,
        }
    }

    #[test]
    fn test_topics() {
        let topics = config().topics();
        assert!(topics.is_subscribe_allowed("robotica/state/lounge/matrix/output2"));
        assert!(topics.is_send_allowed("robotica/command/lounge/matrix"));
        assert!(!topics.is_subscribe_allowed("robotica/state/lounge/matrix/output1"));
    }

    #[test]
    fn test_process_message() {
        let mut controller = config().create_controller();
        assert_eq!(controller.get_display_state(), DisplayState::Unknown);

        controller.process_message(Message::SelectedInput(3));
        assert_eq!(controller.get_display_state(), DisplayState::On);

        controller.process_message(Message::SelectedInput(1));
        assert_eq!(controller.get_display_state(), DisplayState::Off);

        controller.process_message(Message::HardOff);
        assert_eq!(controller.get_display_state(), DisplayState::HardOff);

        controller.process_message(Message::Invalid);
        assert_eq!(controller.get_display_state(), DisplayState::Error);
    }

    #[test]
    fn test_parse_output() {
        fn msg(payload: &str) -> MqttMessage {
            MqttMessage::new(
                "robotica/state/foo/output2",
                payload,
                crate::mqtt::Retain::NoRetain,
                crate::mqtt::QoS::AtMostOnce,
            )
        }

        assert_eq!(parse_output(msg("3")).unwrap(), Message::SelectedInput(3));
        assert_eq!(parse_output(msg("HARD_OFF")).unwrap(), Message::HardOff);
        assert_eq!(parse_output(msg("junk")).unwrap(), Message::Invalid);
    }
}
