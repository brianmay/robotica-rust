//! Shared amber car types
use serde::Deserialize;

use super::combined;

/// Charge request
#[derive(Copy, Clone, Eq, PartialEq, Debug, Deserialize)]
#[serde(tag = "type", content = "value")]
#[serde(rename_all = "snake_case")]
pub enum ChargeRequest {
    /// Charge to a specific level
    ChargeTo(u8),
    /// Do not touch the charge level
    Manual,
}

impl std::fmt::Display for ChargeRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChargeRequest::ChargeTo(level) => write!(f, "charge to {level}%"),
            ChargeRequest::Manual => write!(f, "manual charge"),
        }
    }
}

/// Charge state
#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct State {
    /// The current battery level
    pub battery_level: u8,
    /// The minimum charge level for tomorrow
    pub min_charge_tomorrow: u8,

    /// The combined state
    #[serde(flatten)]
    pub combined: combined::State<ChargeRequest>,
}

impl State {
    /// Get the result of the charge request
    #[must_use]
    pub const fn get_result(&self) -> &ChargeRequest {
        self.combined.get_result()
    }
}
