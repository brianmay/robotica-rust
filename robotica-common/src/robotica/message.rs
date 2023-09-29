//! A message to be sent to the audio system

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use super::audio::MessagePriority;

/// The audience of a message
// FIXME: This is site specific
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[allow(dead_code)]
pub enum MessageAudience {
    /// Message goes to everyone.
    #[default]
    Everyone,

    /// Message goes to Brian only.
    Brian {
        /// Message is private for Brian only
        private: bool,
    },

    /// Message goes to the twins only.
    Twins,
}

impl Display for MessageAudience {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageAudience::Everyone => write!(f, "Everyone"),
            MessageAudience::Brian { private } => {
                if *private {
                    write!(f, "Brian (private)")
                } else {
                    write!(f, "Brian")
                }
            }
            MessageAudience::Twins => write!(f, "Twins"),
        }
    }
}

/// A HA audio command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The title of the message.
    pub title: String,

    /// The message to send.
    pub body: String,

    /// The priority of the message
    pub priority: MessagePriority,

    /// The audience of the message
    #[serde(default)]
    pub audience: MessageAudience,

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
        audience: MessageAudience,
    ) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            priority,
            audience,
            flash_lights: false,
        }
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let body = &self.body;
        let flash_lights = if self.flash_lights {
            " and flash lights"
        } else {
            ""
        };
        write!(f, "say \"{body}\"{flash_lights}")
    }
}
