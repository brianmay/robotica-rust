//! A message to be sent to the audio system

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use super::audio::MessagePriority;

/// The audience for a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Audience(String);

impl Audience {
    /// Create a new audience
    pub fn new(audience: impl Into<String>) -> Self {
        Self(audience.into())
    }
}

impl Display for Audience {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&Audience> for Audience {
    fn from(audience: &Audience) -> Self {
        audience.clone()
    }
}

impl From<&str> for Audience {
    fn from(audience: &str) -> Self {
        Self(audience.to_string())
    }
}

impl From<String> for Audience {
    fn from(audience: String) -> Self {
        Self(audience)
    }
}

/// A HA audio command
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    /// The title of the message.
    pub title: String,

    /// The message to send.
    pub body: String,

    /// The priority of the message
    pub priority: MessagePriority,

    /// The audience of the message
    pub audience: Audience,

    /// Should we flash the lights?
    #[serde(default)]
    pub flash_lights: bool,
}

impl Message {
    /// Create a new message
    pub fn new(
        title: impl Into<String>,
        body: impl Into<String>,
        priority: MessagePriority,
        audience: impl Into<Audience>,
    ) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            priority,
            audience: audience.into(),
            flash_lights: false,
        }
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let audience = &self.audience;
        let body = &self.body;
        let flash_lights = if self.flash_lights {
            " and flash lights"
        } else {
            ""
        };
        write!(f, "tell {audience} \"{body}\"{flash_lights}")
    }
}
