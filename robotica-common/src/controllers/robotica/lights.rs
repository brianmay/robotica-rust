//! A robotica light controller

use crate::{
    mqtt::{topics::Topics, Json, MqttMessage},
    robotica::{
        commands::Command,
        lights::{self, LightCommand, PowerState, SceneName},
    },
};
use serde::{Deserialize, Serialize};

use super::super::{
    get_display_state_for_action, get_press_on_or_off, mqtt_command_vec, Action, ConfigTrait,
    ControllerTrait, DisplayState, ParseError, Subscription, TurnOnOff,
};

/// The configuration for a light controller
#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
pub struct Config {
    /// The topic substring for the light
    pub topic_substr: String,

    /// The action to take when the light is clicked
    pub action: Action,

    /// The scene to use for the light
    pub scene: SceneName,
}

impl Config {
    /// The topic that reports the current scene.
    #[must_use]
    pub fn scene_topic(&self) -> String {
        format!("robotica/state/{}/scene", self.topic_substr)
    }

    /// The topic that reports the power state.
    #[must_use]
    pub fn power_topic(&self) -> String {
        format!("robotica/state/{}/power", self.topic_substr)
    }

    /// The topic that commands the light.
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
            scene: None,
            power: None,
        }
    }

    fn topics(&self) -> Topics {
        let mut topics = Topics::default();
        topics.subscribe_exact(self.scene_topic());
        topics.subscribe_exact(self.power_topic());
        topics.send_exact(self.command_topic());
        topics
    }
}

/// A typed message from a robotica light
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Message {
    /// The current scene
    Scene(SceneName),

    /// The power state
    Power(PowerState),
}

fn parse_scene(msg: MqttMessage) -> Result<Message, ParseError> {
    let scene: SceneName = msg.try_into()?;
    Ok(Message::Scene(scene))
}

fn parse_power(msg: MqttMessage) -> Result<Message, ParseError> {
    let Json(power): Json<PowerState> = msg.try_into()?;
    Ok(Message::Power(power))
}

/// The controller for a light
pub struct Controller {
    config: Config,
    scene: Option<SceneName>,
    power: Option<lights::PowerState>,
}

impl ControllerTrait for Controller {
    type Message = Message;

    fn subscriptions(&self) -> Vec<Subscription<Message>> {
        vec![
            Subscription {
                topic: self.config.scene_topic(),
                parse: parse_scene,
            },
            Subscription {
                topic: self.config.power_topic(),
                parse: parse_power,
            },
        ]
    }

    fn process_message(&mut self, message: Message) {
        match message {
            Message::Scene(scene) => {
                self.scene = Some(scene);
            }
            Message::Power(power) => {
                self.power = Some(power);
            }
        }
    }

    fn process_disconnected(&mut self) {
        self.scene = None;
        self.power = None;
    }

    fn get_display_state(&self) -> DisplayState {
        let state = get_display_state_internal(self);
        let action = self.config.action;
        get_display_state_for_action(state, action)
    }

    fn get_press_commands(&self) -> Vec<MqttMessage> {
        let display_state = self.get_display_state();
        let action = match get_press_on_or_off(display_state, self.config.action) {
            TurnOnOff::TurnOn => LightCommand::TurnOn {
                scene: self.config.scene.clone(),
            },
            TurnOnOff::TurnOff => LightCommand::TurnOff,
        };

        let payload = Json(Command::Light(action));
        let topic = self.config.command_topic();
        mqtt_command_vec(&topic, &payload)
    }

    fn get_action(&self) -> Action {
        self.config.action
    }
}

fn get_display_state_internal(lb: &Controller) -> DisplayState {
    let scene = lb.scene.as_ref();

    let off = scene == Some(&SceneName::new("off"));
    let scene_selected = scene.is_some_and(|scene| *scene == lb.config.scene);

    match lb.power {
        None => DisplayState::Unknown,
        Some(PowerState::Offline) => DisplayState::HardOff,
        Some(PowerState::Off) if !off && scene_selected => DisplayState::AutoOff,
        Some(PowerState::On | PowerState::Off) if scene_selected => DisplayState::On,
        Some(PowerState::On | PowerState::Off) => DisplayState::Off,
    }
}

/// The type used to represent a priority of a light scene
pub type Priority = i32;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn config() -> Config {
        Config {
            topic_substr: "lounge/light".to_string(),
            action: Action::Toggle,
            scene: SceneName::new("bright"),
        }
    }

    #[test]
    fn test_topics() {
        let topics = config().topics();
        assert!(topics.is_subscribe_allowed("robotica/state/lounge/light/scene"));
        assert!(topics.is_subscribe_allowed("robotica/state/lounge/light/power"));
        assert!(topics.is_send_allowed("robotica/command/lounge/light"));
        assert!(!topics.is_subscribe_allowed("robotica/state/lounge/light/status"));
    }

    #[test]
    fn test_process_message() {
        let mut controller = config().create_controller();
        assert_eq!(controller.get_display_state(), DisplayState::Unknown);

        controller.process_message(Message::Scene(SceneName::new("bright")));
        controller.process_message(Message::Power(PowerState::On));
        assert_eq!(controller.get_display_state(), DisplayState::On);

        controller.process_message(Message::Scene(SceneName::new("off")));
        assert_eq!(controller.get_display_state(), DisplayState::Off);

        controller.process_message(Message::Power(PowerState::Offline));
        assert_eq!(controller.get_display_state(), DisplayState::HardOff);
    }
}
