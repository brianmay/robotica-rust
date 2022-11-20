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

/// The display state of a button
#[allow(dead_code)]
#[derive(std::cmp::Eq, std::cmp::PartialEq, Copy, Clone, Debug)]
pub enum DisplayState {
    /// The device is off and cannot be turned on
    HardOff,

    /// The device encountered an error
    Error,

    /// The state of the device is unknown
    Unknown,

    /// The device is On
    On,

    /// The device in on auto, but currently off
    AutoOff,

    /// The device if Off
    Off,
}

impl Display for DisplayState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DisplayState::HardOff => write!(f, "Hard Off"),
            DisplayState::Error => write!(f, "Error"),
            DisplayState::Unknown => write!(f, "Unknown"),
            DisplayState::On => write!(f, "On"),
            DisplayState::AutoOff => write!(f, "Auto Off"),
            DisplayState::Off => write!(f, "Off"),
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

    /// Get the name of this controller
    // fn get_name(&self) -> String;

    /// Get the action to perform when the button is pressed
    fn get_action(&self) -> Action;
}

/// Change the display state based on the action for this controller
#[must_use]
const fn get_display_state_for_action(state: DisplayState, action: Action) -> DisplayState {
    match action {
        Action::TurnOn | Action::Toggle => state,
        Action::TurnOff => match state {
            DisplayState::HardOff => DisplayState::HardOff,
            DisplayState::Error => DisplayState::Error,
            DisplayState::Unknown => DisplayState::Unknown,
            DisplayState::On | DisplayState::AutoOff => DisplayState::Off,
            DisplayState::Off => DisplayState::On,
        },
    }
}

enum TurnOnOff {
    TurnOn,
    TurnOff,
}

#[must_use]
const fn get_press_on_or_off(state: DisplayState, action: Action) -> TurnOnOff {
    match action {
        Action::TurnOn => TurnOnOff::TurnOn,
        Action::TurnOff => TurnOnOff::TurnOff,
        Action::Toggle => match state {
            DisplayState::HardOff
            | DisplayState::Error
            | DisplayState::Unknown
            | DisplayState::Off => TurnOnOff::TurnOn,
            DisplayState::On | DisplayState::AutoOff => TurnOnOff::TurnOff,
        },
    }
}
