//! Define button icons

use super::controllers::DisplayState;

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
            DisplayState::AutoOff => "auto",
            DisplayState::Off => "off",
        };
        format!("/images/{}_{}.svg", self.name, version)
    }
}
