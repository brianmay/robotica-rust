use log::error;

use super::{
    get_display_state_for_action, Action, Command, CommonConfig, ConfigTrait, ControllerTrait,
    DisplayState, Icon, Label, Subscription,
};

#[derive(Clone)]
pub struct Config {
    pub c: CommonConfig,
    pub play_list: String,
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
    play_list: Option<String>,
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
        State { play_list: None }
    }

    fn get_subscriptions(&self) -> Vec<Subscription> {
        let mut result: Vec<Subscription> = Vec::new();
        let config = &self.config;

        let p = ["state", &config.c.topic_substr, "play_list"];
        let s = Subscription {
            topic: topic(&p),
            label: ButtonStateMsgType::PlayList as u32,
        };
        result.push(s);

        result
    }

    fn process_message(&self, label: Label, data: String, state: &mut State) {
        match label.try_into() {
            Ok(ButtonStateMsgType::PlayList) => state.play_list = Some(data),

            _ => error!("Invalid message label {}", label),
        }
    }

    fn process_disconnected(&self, state: &mut State) {
        state.play_list = None;
    }

    fn get_display_state(&self, state: &State) -> DisplayState {
        let play_list = state.play_list.as_deref();
        let state = match play_list {
            None => DisplayState::Unknown,
            Some("ERROR") => DisplayState::Error,
            Some("STOP") => DisplayState::Off,
            Some(pl) if pl == self.config.play_list => DisplayState::On,
            _ => DisplayState::OnOther,
        };

        let action = &self.config.c.action;
        get_display_state_for_action(state, action)
    }

    fn get_press_commands(&self, state: &State) -> Vec<Command> {
        let play = match self.config.c.action {
            Action::TurnOn => true,
            Action::TurnOff => false,
            Action::Toggle => {
                let display_state = self.get_display_state(state);
                !matches!(display_state, DisplayState::On)
            }
        };

        let payload = if play {
            serde_json::json!({
                "music": {"play_list": self.config.play_list}
            })
        } else {
            serde_json::json!({
                "music": {"stop": true}
            })
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
    PlayList,
}

impl TryFrom<u32> for ButtonStateMsgType {
    type Error = ();

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            x if x == ButtonStateMsgType::PlayList as u32 => Ok(ButtonStateMsgType::PlayList),
            _ => Err(()),
        }
    }
}
