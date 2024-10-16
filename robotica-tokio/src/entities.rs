//! Entities represent a device in the system.
use serde::{Deserialize, Serialize};

/// A unique identifier for an entity
#[derive(Debug, Serialize, Deserialize, Clone)]
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
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
