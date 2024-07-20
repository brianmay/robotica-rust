//! A robotica light controller

use crate::{
    mqtt::{Json, MqttMessage},
    robotica::{
        commands::Command,
        lights::{self, LightCommand, PowerState, SceneName},
    },
};
use serde::{Deserialize, Serialize};
use tracing::error;

use super::super::{
    get_display_state_for_action, get_press_on_or_off, mqtt_command_vec, Action, ConfigTrait,
    ControllerTrait, DisplayState, Label, Subscription, TurnOnOff,
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

impl ConfigTrait for Config {
    type Controller = Controller;

    fn create_controller(&self) -> Controller {
        Controller {
            config: self.clone(),
            scene: None,
            power: None,
        }
    }
}

/// The controller for a light
pub struct Controller {
    config: Config,
    scene: Option<SceneName>,
    power: Option<lights::PowerState>,
}

fn topic(parts: &[&str]) -> String {
    parts.join("/")
}

impl ControllerTrait for Controller {
    fn get_subscriptions(&self) -> Vec<Subscription> {
        let mut result: Vec<Subscription> = Vec::new();
        let config = &self.config;

        let p = ["state", &config.topic_substr, "scene"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::Scene as u32,
        };
        result.push(s);

        let p = ["state", &config.topic_substr, "power"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::Power as u32,
        };
        result.push(s);

        result
    }

    fn process_message(&mut self, label: Label, data: MqttMessage) {
        match label.try_into() {
            Ok(ButtonStateMsgType::Scene) => self.scene = Some(data.into()),
            Ok(ButtonStateMsgType::Power) => match data.try_into() {
                Ok(Json(state)) => self.power = Some(state),
                Err(e) => error!("Invalid power value: {e}"),
            },

            Err(()) => error!("Invalid message label {}", label),
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
        let topic = format!("command/{}", self.config.topic_substr);
        mqtt_command_vec(&topic, &payload)
    }

    fn get_action(&self) -> Action {
        self.config.action
    }
}

fn get_display_state_internal(lb: &Controller) -> DisplayState {
    let scene = lb.scene.as_ref();

    let off = scene == Some(&SceneName::new("off"));
    let scene_selected = scene.map_or(false, |scene| *scene == lb.config.scene);

    match lb.power {
        None => DisplayState::Unknown,
        Some(PowerState::Offline) => DisplayState::HardOff,
        Some(PowerState::Off) if !off && scene_selected => DisplayState::AutoOff,
        Some(PowerState::On | PowerState::Off) if scene_selected => DisplayState::On,
        Some(PowerState::On | PowerState::Off) => DisplayState::Off,
    }
}

enum ButtonStateMsgType {
    Scene,
    Power,
}

impl TryFrom<u32> for ButtonStateMsgType {
    type Error = ();

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            x if x == ButtonStateMsgType::Scene as u32 => Ok(ButtonStateMsgType::Scene),
            x if x == ButtonStateMsgType::Power as u32 => Ok(ButtonStateMsgType::Power),
            _ => Err(()),
        }
    }
}

/// The type used to represent a priority of a light scene
pub type Priority = i32;
