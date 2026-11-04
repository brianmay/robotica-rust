//! Controllers are used to control that state of the buttons
use std::fmt::{Display, Formatter};

use crate::mqtt::{topics::Topics, BoolError, JsonError, MqttMessage, MqttSerializer, QoS, Retain};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::error;

pub mod robotica;
pub mod tasmota;
pub mod zwave;

/// The action to happen when a button is pressed.
#[derive(Copy, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[allow(dead_code)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// The button should turn on the device
    TurnOn,

    /// The button should turn off the device
    TurnOff,

    /// The button should toggle the state of the device
    Toggle,
}

/// An error parsing a controller message from an MQTT message.
#[derive(Error, Debug)]
#[error("failed to parse MQTT message: {0}")]
pub struct ParseError(String);

impl ParseError {
    /// Create a new parse error with a message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<JsonError> for ParseError {
    fn from(err: JsonError) -> Self {
        Self(err.to_string())
    }
}

impl From<std::str::Utf8Error> for ParseError {
    fn from(err: std::str::Utf8Error) -> Self {
        Self(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for ParseError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        Self(err.to_string())
    }
}

impl From<BoolError> for ParseError {
    fn from(err: BoolError) -> Self {
        Self(err.to_string())
    }
}

impl From<crate::robotica::switch::PowerError> for ParseError {
    fn from(err: crate::robotica::switch::PowerError) -> Self {
        Self(err.to_string())
    }
}

/// A function that parses an MQTT message into a controller message.
pub type ParseFn<M> = fn(MqttMessage) -> Result<M, ParseError>;

/// A typed subscription for a controller.
///
/// Each subscription pairs an MQTT topic with the function that parses
/// messages received on that topic into the controller's message type, so
/// the topic and the code that decodes it can never fall out of sync.
#[derive(Clone)]
pub struct Subscription<M> {
    /// The topic to subscribe to.
    pub topic: String,

    /// Parse a message received on this topic into a controller message.
    pub parse: ParseFn<M>,
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

    /// Get the MQTT topics this controller subscribes to and sends on.
    fn topics(&self) -> Topics;
}

/// The trait to define a controller
///
/// Controllers process typed messages. Each subscription returned by
/// [`ControllerTrait::subscriptions`] carries the parser for its topic, and
/// [`ControllerTrait::process_message`] matches on the resulting message
/// type exhaustively, so a new subscription or message variant that is not
/// handled is a compile error rather than a silent misroute.
pub trait ControllerTrait {
    /// The typed message this controller processes, parsed from its subscriptions.
    type Message;

    /// Get the subscriptions (topic and parser) for this controller
    fn subscriptions(&self) -> Vec<Subscription<Self::Message>>;

    /// Process an disconnected message
    fn process_disconnected(&mut self);

    /// Process an incoming message
    fn process_message(&mut self, message: Self::Message);

    /// Get the current display state for this controller
    fn get_display_state(&self) -> DisplayState;

    /// Get the commands to be executed when button is pressed
    fn get_press_commands(&self) -> Vec<MqttMessage>;

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

#[must_use]
fn mqtt_command(topic: &str, payload: &impl MqttSerializer) -> Option<MqttMessage> {
    payload
        .serialize(topic, Retain::NoRetain, QoS::ExactlyOnce)
        .map_or_else(
            |_| {
                error!("Failed to serialize payload for topic {}", topic);
                None
            },
            Some,
        )
}

#[must_use]
fn mqtt_command_vec(topic: &str, payload: &impl MqttSerializer) -> Vec<MqttMessage> {
    let msg = mqtt_command(topic, payload);
    msg.map_or_else(std::vec::Vec::new, |msg| vec![msg])
}
