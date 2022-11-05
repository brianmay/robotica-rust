//! Structures and functions for the Anavi Thermometer.
use std::fmt::Display;

use serde::Deserialize;

/// A temperature reading from the Anavi Thermometer.
#[derive(Deserialize)]
pub struct Temperature {
    temperature: f32,
}

impl Display for Temperature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}℃", self.temperature)
    }
}

impl TryFrom<String> for Temperature {
    type Error = serde_json::Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        serde_json::from_str(&s)
    }
}

/// A humidity reading from the Anavi Thermometer.
#[derive(Deserialize)]
pub struct Humidity {
    humidity: f32,
}

impl Display for Humidity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}%", self.humidity)
    }
}

impl TryFrom<String> for Humidity {
    type Error = serde_json::Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        serde_json::from_str(&s)
    }
}
