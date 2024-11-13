//! Entities represent a device in the system.
use serde::{Deserialize, Serialize};

/// A unique identifier for an entity
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Id(String);

impl Id {
    /// Create a new identifier
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the id as a str
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the MQTT state topic for the entity
    #[must_use]
    pub fn get_state_topic(&self, name: &str) -> String {
        if name.is_empty() {
            format!("robotica/state/{}", self.0)
        } else {
            format!("robotica/state/{}/{name}", self.0)
        }
    }

    /// Get the MQTT command topic for the entity
    #[must_use]
    pub fn get_command_topic(&self, name: &str) -> String {
        if name.is_empty() {
            format!("robotica/command/{}", self.0)
        } else {
            format!("robotica/command/{}/{name}", self.0)
        }
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
