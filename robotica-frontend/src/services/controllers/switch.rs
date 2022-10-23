use log::error;

use super::{
    get_display_state_for_action, Action, Command, CommonConfig, ConfigTrait, ControllerTrait,
    DisplayState, Icon, Label, Subscription,
};

#[derive(Clone)]
pub struct Config {
    pub c: CommonConfig,
}

impl ConfigTrait for Config {
    type Controller = Controller;
    type State = State;

    fn create_controller(&self) -> Controller {
        Controller::new(self)
    }
}

pub struct Controller {
    config: Config,
}

pub struct State {
    power: Option<String>,
}

impl Controller {
    pub fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
        }
    }
}

fn topic(parts: &[&str]) -> String {
    parts.join("/")
}

impl ControllerTrait for Controller {
    type State = State;

    fn new_state(&self) -> Self::State {
        State { power: None }
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

        result
    }

    fn process_message(&self, label: Label, data: String, state: &mut State) {
        match label.try_into() {
            Ok(ButtonStateMsgType::Power) => state.power = Some(data),

            _ => error!("Invalid message label {}", label),
        }
    }

    fn process_disconnected(&self, state: &mut State) {
        state.power = None;
    }

    fn get_display_state(&self, state: &State) -> DisplayState {
        let power = state.power.as_deref();

        let state = match power {
            None => DisplayState::Unknown,
            Some("HARD_OFF") => DisplayState::HardOff,
            Some("ON") => DisplayState::On,
            Some("OFF") => DisplayState::Off,
            _ => DisplayState::Error,
        };

        let action = &self.config.c.action;
        get_display_state_for_action(state, action)
    }

    fn get_press_commands(&self, state: &State) -> Vec<Command> {
        let mut payload = serde_json::json!({});

        match self.config.c.action {
            Action::TurnOn => payload["action"] = serde_json::json!("turn_on"),
            Action::TurnOff => payload["action"] = serde_json::json!("turn_off"),
            Action::Toggle => {
                let display_state = self.get_display_state(state);
                if let DisplayState::On = display_state {
                    payload["action"] = serde_json::json!("turn_off");
                } else {
                    payload["action"] = serde_json::json!("turn_on");
                }
            }
        };

        let topic = format!("command/{}", self.config.c.topic_substr);
        let command = Command { topic, payload };

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

enum ButtonStateMsgType {
    Power,
}

impl TryFrom<u32> for ButtonStateMsgType {
    type Error = ();

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            x if x == ButtonStateMsgType::Power as u32 => Ok(ButtonStateMsgType::Power),
            _ => Err(()),
        }
    }
}
