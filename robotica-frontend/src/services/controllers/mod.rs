//! Controllers are used to control that state of the buttons
use std::fmt::{Display, Formatter};

pub mod hdmi;
pub mod lights;
pub mod music;
pub mod switch;

/// A label is used to identity an incoming MQTT message
pub type Label = u32;

/// An outgoing MQTT message from a controller
pub struct Command {
    topic: String,
    payload: serde_json::Value,
}

impl Command {
    /// Get the topic of a command
    #[must_use]
    pub fn get_topic(&self) -> &str {
        &self.topic
    }

    /// Get the payload of a command
    #[must_use]
    pub fn get_payload(&self) -> String {
        self.payload.to_string()
    }
}

/// The action to happen when a button is pressed.
#[derive(Copy, Clone, Eq, PartialEq)]
#[allow(dead_code)]
pub enum Action {
    /// The button should turn on the device
    TurnOn,

    /// The button should turn off the device
    TurnOff,

    /// The button should toggle the state of the device
    Toggle,
}

/// A MQTT subscription request from a controller
#[derive(Clone)]
pub struct Subscription {
    /// The topic to subscribe to
    pub topic: String,

    /// The label to use when the topic is received
    pub label: Label,
}

/// The ddisplay state of a button
#[allow(dead_code)]
#[derive(std::cmp::Eq, std::cmp::PartialEq, Clone, Debug)]
pub enum DisplayState {
    /// The device is off and cannot be turned on
    HardOff,

    /// The device encountered an error
    Error,

    /// The state of the device is unknown
    Unknown,

    /// The device is On
    On,

    /// The device if Off
    Off,

    /// The device is On, but not for another function
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

/// The trait to define a configuration for a controller
pub trait ConfigTrait {
    /// This is the controller that will be used to process the messages
    type Controller: ControllerTrait;

    /// Get the controller for this configuration
    fn create_controller(&self) -> Self::Controller;
}

/// Define an Icon for a button
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Icon {
    name: String,
}

impl Icon {
    /// Create a new icon
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }

    /// Get the URL to the icon
    #[must_use]
    pub fn to_href(&self, state: &DisplayState) -> String {
        let version = match state {
            DisplayState::HardOff | DisplayState::Error | DisplayState::Unknown => "error",
            DisplayState::On => "on",
            DisplayState::Off => "off",
            DisplayState::OnOther => "other",
        };
        format!("/images/{}_{}.svg", self.name, version)
    }
}

/// The trait to define a controller
pub trait ControllerTrait {
    /// Get the subscriptions for this controller
    fn get_subscriptions(&self) -> Vec<Subscription>;

    /// Process an disconnected message
    fn process_disconnected(&mut self);

    /// Process an incoming message
    fn process_message(&mut self, label: Label, data: String);

    /// Get the current display state for this controller
    fn get_display_state(&self) -> DisplayState;

    /// Get the commands to be executed when button is pressed
    fn get_press_commands(&self) -> Vec<Command>;

    /// Get the icon for this controller
    fn get_icon(&self) -> Icon;

    /// Get the name of this controller
    fn get_name(&self) -> String;

    /// Get the action to perform when the button is pressed
    fn get_action(&self) -> Action;
}

/// Change the display state based on the action for this controller
#[must_use]
pub const fn get_display_state_for_action(state: DisplayState, action: &Action) -> DisplayState {
    match action {
        Action::TurnOn | Action::Toggle => state,
        Action::TurnOff => match state {
            DisplayState::HardOff => DisplayState::HardOff,
            DisplayState::Error => DisplayState::Error,
            DisplayState::Unknown => DisplayState::Unknown,
            DisplayState::On | DisplayState::OnOther => DisplayState::Off,
            DisplayState::Off => DisplayState::On,
        },
    }
}
