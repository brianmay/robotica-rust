//! Structures and functions for the Anavi Thermometer.
use std::fmt::Display;

use serde::Deserialize;

use crate::mqtt::MqttMessage;

/// Get the reading for a measurement
pub trait GetReading {
    /// Get the reading for a measurement
    fn get_reading(&self) -> f64;
}

/// A temperature reading from the Anavi Thermometer.
#[derive(Deserialize, Copy, Clone)]
pub struct Temperature {
    temperature: f64,
}

impl Display for Temperature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}℃", self.temperature)
    }
}

impl TryFrom<MqttMessage> for Temperature {
    type Error = serde_json::Error;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        serde_json::from_str(&msg.payload)
    }
}

impl GetReading for Temperature {
    fn get_reading(&self) -> f64 {
        self.temperature
    }
}

/// A humidity reading from the Anavi Thermometer.
#[derive(Deserialize, Copy, Clone)]
pub struct Humidity {
    humidity: f64,
}

impl Display for Humidity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}%", self.humidity)
    }
}

impl TryFrom<MqttMessage> for Humidity {
    type Error = serde_json::Error;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        serde_json::from_str(&msg.payload)
    }
}

impl GetReading for Humidity {
    fn get_reading(&self) -> f64 {
        self.humidity
    }
}
