//! Audio player Service

use serde::{Deserialize, Serialize};

use super::tasks::SubTask;

/// The current volume levels
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VolumeState {
    /// The volume level for music
    pub music: u8,
    /// The volume level for messages
    pub message: u8,
}

impl Default for VolumeState {
    fn default() -> Self {
        Self {
            music: 90,
            message: 90,
        }
    }
}

/// The current state of the audio player
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct State {
    /// The current playlist
    pub play_list: Option<String>,
    /// The current message
    pub error: Option<String>,
    /// The current volume levels
    pub volume: VolumeState,
}

/// A command to play some music
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicCommand {
    /// The playlist to play.
    pub play_list: Option<String>,

    /// Should we stop playing music?
    pub stop: Option<bool>,
}

/// A command to change the volumes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeCommand {
    /// The music volume to set.
    pub music: Option<u8>,

    /// The message volume to set.
    pub message: Option<u8>,
}

/// The priority of a message
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum MessagePriority {
    /// The message is urgent and should be delivered immediately.
    Urgent,

    /// The message is not important.
    Low,

    /// The message is important and should be delivered during the day if allowed.
    DaytimeOnly,
}

/// A message to send
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    /// Legacy format: just a string
    String(String),

    /// New format: a struct with more details
    ///
    /// This format is not supported by old code and should not be used yet.
    Details {
        /// The title of the message.
        title: String,

        /// The message to send.
        body: String,

        /// The priority of the message.
        priority: MessagePriority,
    },
}

impl Message {
    /// Create a new message
    pub fn new(
        title: impl Into<String>,
        body: impl Into<String>,
        priority: MessagePriority,
    ) -> Self {
        Self::Details {
            title: title.into(),
            body: body.into(),
            priority,
        }
    }

    /// What is the title?
    #[must_use]
    pub fn title(&self) -> &str {
        match self {
            Self::String(_) => "No title",
            Self::Details { title, .. } => title,
        }
    }

    /// What is the message?
    #[must_use]
    pub fn body(&self) -> &str {
        match self {
            Self::String(s) => s,
            Self::Details { body, .. } => body,
        }
    }

    /// What is the priority?
    #[must_use]
    pub const fn priority(&self) -> MessagePriority {
        match self {
            Self::String(_) => MessagePriority::Low,
            Self::Details { priority, .. } => *priority,
        }
    }

    /// Get owned body
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn into_body(self) -> String {
        match self {
            Self::String(s) => s,
            Self::Details { body, .. } => body,
        }
    }

    /// Get owned strings
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn into_owned(self) -> (String, String, MessagePriority) {
        match self {
            Self::String(s) => ("No title".to_string(), s, MessagePriority::Low),
            Self::Details {
                title,
                body,
                priority,
            } => (title, body, priority),
        }
    }

    /// Turn into a HA message
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn into_ha_message(self) -> HaMessageCommand {
        match self {
            Self::String(s) => HaMessageCommand {
                title: "No title".to_string(),
                body: s,
                priority: MessagePriority::Low,
            },
            Self::Details {
                title,
                body,
                priority,
            } => HaMessageCommand {
                title,
                body,
                priority,
            },
        }
    }

    /// Decide if we should play this message
    #[cfg(feature = "chrono")]
    #[must_use]
    pub fn should_play(&self, now: chrono::DateTime<chrono::Local>, enabled: bool) -> bool {
        use chrono::Timelike;
        let day_hour = matches!(now.hour(), 7..=21);

        let priority = self.priority();
        #[allow(clippy::match_same_arms)]
        match (priority, day_hour, enabled) {
            (MessagePriority::Urgent, _, _) => true,
            (MessagePriority::Low, _, true) => true,
            (MessagePriority::Low, _, _) => false,
            (MessagePriority::DaytimeOnly, true, true) => true,
            (MessagePriority::DaytimeOnly, _, _) => false,
        }
    }
}

/// An audio command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCommand {
    /// The title of the message.
    #[serde(rename = "title")]
    pub legacy_title: Option<String>,

    /// The message to send.
    pub message: Option<Message>,

    /// The sounds to play.
    pub sound: Option<String>,

    /// The music to play.
    pub music: Option<MusicCommand>,

    /// The volume to set.
    pub volume: Option<VolumeCommand>,

    /// Pre tasks to execute before playing the message.
    pub pre_tasks: Option<Vec<SubTask>>,

    /// Post tasks to execute after playing the message.
    pub post_tasks: Option<Vec<SubTask>>,
}

impl AudioCommand {
    /// What is the title?
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        if let Some(Message::Details { title, .. }) = &self.message {
            Some(title)
        } else {
            self.legacy_title.as_deref()
        }
    }
}

/// A HA audio command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaMessageCommand {
    /// The title of the message.
    pub title: String,

    /// The message to send.
    #[serde(rename = "message")]
    pub body: String,

    /// The priority of the message
    pub priority: MessagePriority,
}
