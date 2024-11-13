//! Shared front end configuration

use serde::{Deserialize, Serialize};

use crate::{
    controllers::{
        robotica::{hdmi, lights, music, switch},
        tasmota, zwave,
    },
    robotica::entities::Id,
};

/// Configuration for a button Controller
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ControllerConfig {
    /// This is a HDMI button
    Hdmi(hdmi::Config),

    /// This is a light button
    Light(lights::Config),

    /// This is a music button
    Music(music::Config),

    /// This is a switch button
    Switch(switch::Config),

    /// This is a zwave button
    Zwave(zwave::Config),

    /// This is a tasmota button
    Tasmota(tasmota::Config),
}

/// An Icon for a button
#[derive(Deserialize, Serialize, Copy, Clone, Eq, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Icon {
    /// This is a fan icon
    Fan,
    /// This is a light icon
    Light,
    /// This is a night icon
    Night,
    /// This is a schedule icon
    Schedule,
    /// This is a selection icon
    Select,
    /// This is a speaker icon
    Speaker,
    /// This is a trumpet icon
    Trumpet,
    /// This is a tv icon
    Tv,
}

/// Configuration for a button
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct ButtonConfig {
    /// The id for this button
    pub id: String,
    /// The controller for this button
    pub controller: ControllerConfig,
    /// The title for this button
    pub title: String,
    /// The icon for this button
    pub icon: Icon,
}

/// Configuration for a button row
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct ButtonRowConfig {
    /// The id for this button row
    pub id: String,
    /// The title for this button row
    pub title: String,
    /// The buttons for this button row
    pub buttons: Vec<ButtonConfig>,
}

/// Configuration for a room
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct RoomConfig {
    /// The id for this room
    pub id: String,
    /// The title for this room
    pub title: String,
    /// The menu for this room
    pub menu: String,
    /// The button rows for this room
    pub rows: Vec<ButtonRowConfig>,
}

/// Configuration for a car

#[derive(Deserialize, Serialize, Clone, Eq, PartialEq, Debug)]
pub struct CarConfig {
    /// The id of the car
    pub id: Id,
    /// The name of the car
    pub title: String,
}

/// Hotwater configuration
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct HotWaterConfig {
    /// The id of the hot water
    pub id: Id,
    /// The name of the hot water
    pub title: String,
}

/// A number of rooms indexed by id
pub type Rooms = Vec<RoomConfig>;

/// Configuration for the UI
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct Config {
    /// The rooms for this UI
    pub rooms: Rooms,

    /// The cars for this UI
    pub cars: Vec<CarConfig>,

    /// The hotwater config for this UI
    pub hot_water: Vec<HotWaterConfig>,

    /// The name of the server in MQTT topics
    pub instance: String,
}
