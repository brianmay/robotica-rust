//! Structures and functions for the Anavi Thermometer.
use std::fmt::Display;

// use chrono::{DateTime, Utc};
use serde::Deserialize;

/// A temperature reading from the Anavi Thermometer.
#[derive(Deserialize, Copy, Clone)]
pub struct Temperature {
    /// The temperature in degrees Celsius.
    pub temperature: f64,
}

impl Display for Temperature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}℃", self.temperature)
    }
}

/// A humidity reading from the Anavi Thermometer.
#[derive(Deserialize, Copy, Clone)]
pub struct Humidity {
    /// The humidity in percent.
    pub humidity: f64,
}

impl Display for Humidity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}%", self.humidity)
    }
}
