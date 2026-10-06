//! A message to be sent to the audio system

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

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

/// The priority of a message
#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MessagePriority {
    /// The message is a routine message.
    #[default]
    Info,

    /// The message is important and should be displayed prominently.
    Important,

    /// The message indicates something went wrong that needs to be fixed.
    Error,

    /// The message indicates a life threatening situation. e.g. building fire, medical emergency.
    Emergency,
}

impl Display for MessagePriority {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "Info"),
            Self::Important => write!(f, "Important"),
            Self::Error => write!(f, "Error"),
            Self::Emergency => write!(f, "Emergency"),
        }
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
        }
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let audience = &self.audience;
        let body = &self.body;
        let priority = &self.priority;
        write!(f, "tell {audience} {priority} \"{body}\"")
    }
}
