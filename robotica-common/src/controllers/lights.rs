//! A robotica light controller

use crate::mqtt::MqttMessage;
use log::error;

use super::{
    get_press_on_or_off, json_command_vec, Action, ConfigTrait, ControllerTrait, DisplayState,
    Label, Subscription, TurnOnOff,
};

/// The configuration for a light controller
#[derive(Clone)]
pub struct Config {
    /// The topic substring for the light
    pub topic_substr: String,

    /// The action to take when the light is clicked
    pub action: Action,

    /// The scene to use for the light
    pub scene: String,

    /// The priority to use for the scene
    pub priority: Priority,
}

impl ConfigTrait for Config {
    type Controller = Controller;

    fn create_controller(&self) -> Controller {
        Controller {
            config: self.clone(),
            power: None,
            scenes: None,
            priorities: None,
        }
    }
}

/// The controller for a light
pub struct Controller {
    config: Config,
    power: Option<String>,
    scenes: Option<Vec<String>>,
    priorities: Option<Vec<Priority>>,
}

fn topic(parts: &[&str]) -> String {
    parts.join("/")
}

impl ControllerTrait for Controller {
    fn get_subscriptions(&self) -> Vec<Subscription> {
        let mut result: Vec<Subscription> = Vec::new();
        let config = &self.config;

        let p = ["state", &config.topic_substr, "power"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::Power as u32,
        };
        result.push(s);

        let p = ["state", &config.topic_substr, "scenes"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::Scenes as u32,
        };
        result.push(s);

        let p = ["state", &config.topic_substr, "priorities"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::Priorities as u32,
        };
        result.push(s);

        result
    }

    fn process_message(&mut self, label: Label, data: String) {
        match label.try_into() {
            Ok(ButtonStateMsgType::Power) => self.power = Some(data),

            Ok(ButtonStateMsgType::Scenes) => match serde_json::from_str(&data) {
                Ok(scenes) => self.scenes = Some(scenes),
                Err(e) => error!("Invalid scenes value {}: {}", data, e),
            },

            Ok(ButtonStateMsgType::Priorities) => match serde_json::from_str(&data) {
                Ok(priorities) => self.priorities = Some(priorities),
                Err(e) => error!("Invalid priorities value {}: {}", data, e),
            },

            _ => error!("Invalid message label {}", label),
        }
    }

    fn process_disconnected(&mut self) {
        self.power = None;
        self.scenes = None;
        self.priorities = None;
    }

    fn get_display_state(&self) -> DisplayState {
        let action = &self.config.action;

        match action {
            Action::Toggle | Action::TurnOn => get_display_state_turn_on(self),
            Action::TurnOff => get_display_state_turn_off(self),
        }
    }

    fn get_press_commands(&self) -> Vec<MqttMessage> {
        let mut payload = serde_json::json!({
            "scene": self.config.scene,
            "priority": self.config.priority,
            "type": "light"
        });

        let display_state = self.get_display_state();
        let action = match get_press_on_or_off(display_state, self.config.action) {
            TurnOnOff::TurnOn => None,
            TurnOnOff::TurnOff => Some("turn_off"),
        };

        if let Some(action) = action {
            payload["action"] = serde_json::json!(action);
        }

        let topic = format!("command/{}", self.config.topic_substr);
        json_command_vec(&topic, &payload)
    }

    fn get_action(&self) -> Action {
        self.config.action
    }
}

fn get_display_state_turn_on(lb: &Controller) -> DisplayState {
    let power = lb.power.as_deref();
    let scenes = lb.scenes.as_deref();
    let scene = &lb.config.scene;

    let scenes_contains = scenes.map_or(false, |scenes| scenes.contains(scene));

    match power {
        None => DisplayState::Unknown,
        Some("HARD_OFF") => DisplayState::HardOff,
        Some("OFF") if scenes_contains => DisplayState::AutoOff,
        _ => match scenes {
            None => DisplayState::Unknown,
            Some(_) if scenes_contains => DisplayState::On,
            Some(_) => DisplayState::Off,
        },
    }
}

fn get_display_state_turn_off(lb: &Controller) -> DisplayState {
    let power = lb.power.as_deref();
    let scenes = lb.scenes.as_deref();
    let priorities = lb.priorities.as_deref();
    let priority = lb.config.priority;

    let scenes_empty = match scenes {
        Some(scenes) if !scenes.is_empty() => false,
        Some(_) | None => true,
    };

    match power {
        None => DisplayState::Unknown,
        Some("HARD_OFF") => DisplayState::HardOff,
        Some("ON") if scenes_empty => DisplayState::Off,
        Some("OFF") if scenes_empty => DisplayState::On,
        _ => match priorities {
            None => DisplayState::Unknown,
            Some(priorities) if priorities.contains(&priority) => DisplayState::Off,
            Some(_) => DisplayState::On,
        },
    }
}

enum ButtonStateMsgType {
    Power,
    Scenes,
    Priorities,
}

impl TryFrom<u32> for ButtonStateMsgType {
    type Error = ();

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            x if x == ButtonStateMsgType::Power as u32 => Ok(ButtonStateMsgType::Power),
            x if x == ButtonStateMsgType::Scenes as u32 => Ok(ButtonStateMsgType::Scenes),
            x if x == ButtonStateMsgType::Priorities as u32 => Ok(ButtonStateMsgType::Priorities),
            _ => Err(()),
        }
    }
}

/// The type used to represent a priority of a light scene
pub type Priority = i32;
