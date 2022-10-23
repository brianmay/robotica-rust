use std::fmt::{Display, Formatter};

pub mod lights;
pub mod music;
pub mod switch;

pub type Label = u32;

pub struct Command {
    topic: String,
    payload: serde_json::Value,
}

impl Command {
    pub fn get_topic(&self) -> &str {
        &self.topic
    }

    pub fn get_payload(&self) -> String {
        self.payload.to_string()
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[allow(dead_code)]
pub enum Action {
    TurnOn,
    TurnOff,
    Toggle,
}

#[derive(Clone)]
pub struct Subscription {
    pub topic: String,
    pub label: Label,
}

#[allow(dead_code)]
#[derive(std::cmp::Eq, std::cmp::PartialEq, Clone, Debug)]
pub enum DisplayState {
    HardOff,
    Error,
    Unknown,
    On,
    Off,
    OnOther,
}

impl Display for DisplayState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DisplayState::HardOff => write!(f, "Hard Off"),
            DisplayState::Error => write!(f, "Error"),
            DisplayState::Unknown => write!(f, "Unknown"),
            DisplayState::On => write!(f, "On"),
            DisplayState::Off => write!(f, "Off"),
            DisplayState::OnOther => write!(f, "On Other"),
        }
    }
}

pub trait ConfigTrait {
    type Controller: ControllerTrait<State = Self::State>;
    type State;

    fn create_controller(&self) -> Self::Controller;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Icon {
    pub on: String,
    pub off: String,
    pub hard_off: String,
    pub on_other: String,
}

impl Icon {
    pub fn to_href(&self, state: &DisplayState) -> String {
        match state {
            DisplayState::HardOff => self.hard_off.clone(),
            DisplayState::Error => self.hard_off.clone(),
            DisplayState::Unknown => self.hard_off.clone(),
            DisplayState::On => self.on.clone(),
            DisplayState::Off => self.off.clone(),
            DisplayState::OnOther => self.on_other.clone(),
        }
    }
}

#[derive(Clone)]
pub struct CommonConfig {
    pub name: String,
    pub topic_substr: String,
    pub action: Action,
    pub icon: Icon,
}

pub trait ControllerTrait {
    type State;

    // fn new(config: impl ConfigTrait<Controller = Self>) -> Self;
    fn new_state(&self) -> Self::State;
    fn get_subscriptions(&self) -> Vec<Subscription>;
    fn process_disconnected(&self, state: &mut Self::State);
    fn process_message(&self, label: Label, data: String, state: &mut Self::State);
    fn get_display_state(&self, state: &Self::State) -> DisplayState;
    fn get_press_commands(&self, state: &Self::State) -> Vec<Command>;
    fn get_icon(&self) -> Icon;
    fn get_name(&self) -> String;
    fn get_action(&self) -> Action;
}

pub fn get_display_state_for_action(state: DisplayState, action: &Action) -> DisplayState {
    match action {
        Action::TurnOn => state,
        Action::TurnOff => match state {
            DisplayState::HardOff => DisplayState::HardOff,
            DisplayState::Error => DisplayState::Error,
            DisplayState::Unknown => DisplayState::Unknown,
            DisplayState::On => DisplayState::Off,
            DisplayState::Off => DisplayState::On,
            DisplayState::OnOther => DisplayState::Off,
        },
        Action::Toggle => state,
    }
}
