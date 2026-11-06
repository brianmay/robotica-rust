//! Reminder types shared between the backend and the frontend.

use std::fmt::{Display, Formatter};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::message::Message;

/// A unique identifier for a reminder instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ReminderId(String);

impl ReminderId {
    /// Create a new reminder identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the identifier as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ReminderId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ReminderId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for ReminderId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// The status of a reminder.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderStatus {
    /// The reminder is still being sent repeatedly.
    Active,

    /// The reminder has been acknowledged and is no longer sent.
    Acknowledged,

    /// The reminder is suppressed by a silence.
    Silenced,
}

impl Display for ReminderStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "Active"),
            Self::Acknowledged => write!(f, "Acknowledged"),
            Self::Silenced => write!(f, "Silenced"),
        }
    }
}

/// A reminder as published to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderInfo {
    /// The unique identifier of this reminder instance.
    pub id: ReminderId,

    /// The stable identifier used to match silences.
    pub key: String,

    /// The message sent when the reminder is active.
    pub message: Message,

    /// When the reminder was created.
    pub created_at: DateTime<Utc>,

    /// The current status of the reminder.
    pub status: ReminderStatus,

    /// When the matching silence expires, if it is not indefinite.
    pub silenced_until: Option<DateTime<Utc>>,
}

/// A command sent by the frontend to the reminder service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ReminderCommand {
    /// Acknowledge a reminder so that it stops being sent.
    Acknowledge {
        /// The reminder to acknowledge.
        id: ReminderId,
    },

    /// Silence all reminders matching a key.
    Silence {
        /// The reminder key to silence.
        key: String,

        /// How long to silence the reminder for.
        ///
        /// `None` means the silence does not expire.
        #[serde(default)]
        duration_minutes: Option<u32>,
    },

    /// Remove the silences for a key.
    Unsilence {
        /// The reminder key to unsilence.
        key: String,
    },
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::robotica::message::MessagePriority;
    use serde_json::json;

    #[test]
    fn test_reminder_id() {
        let id = ReminderId::new("abc");
        assert_eq!(id.as_str(), "abc");
        assert_eq!(serde_json::to_value(&id).unwrap(), json!("abc"));
    }

    #[test]
    fn test_reminder_info_serialization() {
        let info = ReminderInfo {
            id: ReminderId::new("abc"),
            key: "tesla_1/plug_in".to_string(),
            message: Message::new("Title", "Body", MessagePriority::Error, "brian"),
            created_at: "2026-11-06T00:00:00Z".parse().unwrap(),
            status: ReminderStatus::Active,
            silenced_until: None,
        };

        let value = serde_json::to_value(&info).unwrap();
        assert_eq!(value["status"], json!("active"));
        assert_eq!(value["key"], json!("tesla_1/plug_in"));
        assert_eq!(value["message"]["priority"], json!("Error"));
    }

    #[test]
    fn test_reminder_command_serialization() {
        let command = ReminderCommand::Silence {
            key: "tesla_1/plug_in".to_string(),
            duration_minutes: Some(60),
        };
        assert_eq!(
            serde_json::to_value(&command).unwrap(),
            json!({
                "type": "silence",
                "key": "tesla_1/plug_in",
                "duration_minutes": 60,
            })
        );

        let command = ReminderCommand::Unsilence {
            key: "tesla_1/plug_in".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&command).unwrap(),
            json!({
                "type": "unsilence",
                "key": "tesla_1/plug_in",
            })
        );
    }
}
