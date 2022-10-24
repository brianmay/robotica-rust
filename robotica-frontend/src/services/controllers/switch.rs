use log::error;

use super::{
    get_display_state_for_action, Action, Command, ConfigTrait, ControllerTrait, DisplayState,
    Icon, Label, Subscription,
};

#[derive(Clone)]
pub struct Config {
    pub name: String,
    pub topic_substr: String,
    pub action: Action,
    pub icon: Icon,
}

impl ConfigTrait for Config {
    type Controller = Controller;

    fn create_controller(&self) -> Controller {
        Controller::new(self)
    }
}

pub struct Controller {
    config: Config,
    power: Option<String>,
}

impl Controller {
    pub fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
            power: None,
        }
    }
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

        result
    }

    fn process_message(&mut self, label: Label, data: String) {
        match label.try_into() {
            Ok(ButtonStateMsgType::Power) => self.power = Some(data),

            _ => error!("Invalid message label {}", label),
        }
    }

    fn process_disconnected(&mut self) {
        self.power = None;
    }

    fn get_display_state(&self) -> DisplayState {
        let power = self.power.as_deref();

        let state = match power {
            None => DisplayState::Unknown,
            Some("HARD_OFF") => DisplayState::HardOff,
            Some("ON") => DisplayState::On,
            Some("OFF") => DisplayState::Off,
            _ => DisplayState::Error,
        };

        let action = &self.config.action;
        get_display_state_for_action(state, action)
    }

    fn get_press_commands(&self) -> Vec<Command> {
        let mut payload = serde_json::json!({});

        match self.config.action {
            Action::TurnOn => payload["action"] = serde_json::json!("turn_on"),
            Action::TurnOff => payload["action"] = serde_json::json!("turn_off"),
            Action::Toggle => {
                let display_state = self.get_display_state();
                if let DisplayState::On = display_state {
                    payload["action"] = serde_json::json!("turn_off");
                } else {
                    payload["action"] = serde_json::json!("turn_on");
                }
            }
        };

        let topic = format!("command/{}", self.config.topic_substr);
        let command = Command { topic, payload };

        vec![command]
    }

    fn get_icon(&self) -> Icon {
        self.config.icon.clone()
    }

    fn get_name(&self) -> String {
        self.config.name.clone()
    }

    fn get_action(&self) -> Action {
        self.config.action
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
