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
            DisplayState::OnOther => write!(f, "Other"),
        }
    }
}

pub trait ConfigTrait {
    type Controller: ControllerTrait;

    fn create_controller(&self) -> Self::Controller;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Icon {
    pub name: String,
}

impl Icon {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }

    pub fn to_href(&self, state: &DisplayState) -> String {
        let version = match state {
            DisplayState::HardOff => "hard_off",
            DisplayState::Error => "hard_off",
            DisplayState::Unknown => "hard_off",
            DisplayState::On => "on",
            DisplayState::Off => "off",
            DisplayState::OnOther => "on_other",
        };
        format!("/images/{}_{}.svg", self.name, version)
    }
}

pub trait ControllerTrait {
    fn get_subscriptions(&self) -> Vec<Subscription>;
    fn process_disconnected(&mut self);
    fn process_message(&mut self, label: Label, data: String);
    fn get_display_state(&self) -> DisplayState;
    fn get_press_commands(&self) -> Vec<Command>;
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
