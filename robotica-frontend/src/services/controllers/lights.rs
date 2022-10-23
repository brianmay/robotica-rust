use log::error;

use super::{
    Action, Command, CommonConfig, ConfigTrait, ControllerTrait, DisplayState, Icon, Label,
    Subscription,
};

#[derive(Clone)]
pub struct Config {
    pub c: CommonConfig,
    pub scene: String,
    pub priority: Priority,
}

impl ConfigTrait for Config {
    type Controller = Controller;
    type State = State;

    fn create_controller(&self) -> Controller {
        Controller {
            config: self.clone(),
        }
    }
}

pub struct Controller {
    config: Config,
}

#[derive(Clone)]
pub struct State {
    power: Option<String>,
    scenes: Option<Vec<String>>,
    priorities: Option<Vec<Priority>>,
}

fn topic(parts: &[&str]) -> String {
    parts.join("/")
}

impl ControllerTrait for Controller {
    type State = State;

    // fn new(config: impl ConfigTrait<Controller = Self>) -> Self {
    //     config.create_controller()
    // }

    fn new_state(&self) -> State {
        State {
            power: None,
            scenes: None,
            priorities: None,
        }
    }

    fn get_subscriptions(&self) -> Vec<Subscription> {
        let mut result: Vec<Subscription> = Vec::new();
        let config = &self.config;

        let p = ["state", &config.c.topic_substr, "power"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::Power as u32,
        };
        result.push(s);

        let p = ["state", &config.c.topic_substr, "scenes"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::Scenes as u32,
        };
        result.push(s);

        let p = ["state", &config.c.topic_substr, "priorities"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::Priorities as u32,
        };
        result.push(s);

        result
    }

    fn process_message(&self, label: Label, data: String, state: &mut State) {
        match label.try_into() {
            Ok(ButtonStateMsgType::Power) => state.power = Some(data),

            Ok(ButtonStateMsgType::Scenes) => match serde_json::from_str(&data) {
                Ok(scenes) => state.scenes = Some(scenes),
                Err(e) => error!("Invalid scenes value {}: {}", data, e),
            },

            Ok(ButtonStateMsgType::Priorities) => match serde_json::from_str(&data) {
                Ok(priorities) => state.priorities = Some(priorities),
                Err(e) => error!("Invalid priorities value {}: {}", data, e),
            },

            _ => error!("Invalid message label {}", label),
        }
    }

    fn process_disconnected(&self, state: &mut State) {
        state.power = None;
        state.scenes = None;
        state.priorities = None;
    }

    fn get_display_state(&self, state: &State) -> DisplayState {
        let action = &self.config.c.action;

        match action {
            Action::TurnOn => get_display_state_turn_on(self, state),
            Action::TurnOff => get_display_state_turn_off(self, state),
            Action::Toggle => get_display_state_toggle(self, state),
        }
    }

    fn get_press_commands(&self, state: &State) -> Vec<Command> {
        let mut message = serde_json::json!({
            "scene": self.config.scene,
            "priority": self.config.priority,
        });

        match self.config.c.action {
            Action::TurnOn => {}
            Action::TurnOff => message["action"] = serde_json::json!("turn_off"),
            Action::Toggle => {
                let display_state = self.get_display_state(state);
                if let DisplayState::On = display_state {
                    message["action"] = serde_json::json!("turn_off");
                };
            }
        };

        let topic = format!("command/{}", self.config.c.topic_substr);
        let command = Command {
            topic,
            payload: message,
        };

        vec![command]
    }

    fn get_icon(&self) -> Icon {
        self.config.c.icon.clone()
    }

    fn get_name(&self) -> String {
        self.config.c.name.clone()
    }

    fn get_action(&self) -> Action {
        self.config.c.action
    }
}

fn get_display_state_turn_on(lb: &Controller, state: &State) -> DisplayState {
    let power = state.power.as_deref();
    let scenes = state.scenes.as_deref();
    let scene = &lb.config.scene;

    let scenes_empty = match scenes {
        Some(scenes) if !scenes.is_empty() => false,
        Some(_) => true,
        None => true,
    };

    match power {
        None => DisplayState::Unknown,
        Some("HARD_OFF") => DisplayState::HardOff,
        Some("ON") if scenes_empty => DisplayState::OnOther,
        Some("OFF") if scenes_empty => DisplayState::Off,
        _ => match scenes {
            None => DisplayState::Unknown,
            Some(scenes) if scenes.contains(scene) => DisplayState::On,
            Some(_) if !scenes_empty => DisplayState::OnOther,
            Some(_) => DisplayState::Off,
        },
    }
}

fn get_display_state_turn_off(lb: &Controller, state: &State) -> DisplayState {
    let power = state.power.as_deref();
    let scenes = state.scenes.as_deref();
    let priorities = state.priorities.as_deref();
    let priority = lb.config.priority;

    let scenes_empty = match scenes {
        Some(scenes) if !scenes.is_empty() => false,
        Some(_) => true,
        None => true,
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

fn get_display_state_toggle(lb: &Controller, state: &State) -> DisplayState {
    let power = state.power.as_deref();
    let scenes = state.scenes.as_deref();
    let scene = &lb.config.scene;

    let scenes_empty = match scenes {
        Some(scenes) if !scenes.is_empty() => false,
        Some(_) => true,
        None => true,
    };

    match power {
        None => DisplayState::Unknown,
        Some("HARD_OFF") => DisplayState::HardOff,
        Some("ON") if scenes_empty => DisplayState::OnOther,
        Some("OFF") if scenes_empty => DisplayState::Off,
        _ => match scenes {
            None => DisplayState::Unknown,
            Some(scenes) if scenes.contains(scene) => DisplayState::On,
            Some(_) if !scenes_empty => DisplayState::OnOther,
            Some(_) => DisplayState::Off,
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

pub type Priority = i32;
