//! Audio player Service

use std::fmt::{Display, Formatter};

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MusicCommand {
    /// The playlist to play.
    pub play_list: Option<String>,

    /// Should we stop playing music?
    pub stop: Option<bool>,
}

/// A command to change the volumes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VolumeCommand {
    /// The music volume to set.
    pub music: Option<u8>,

    /// The message volume to set.
    pub message: Option<u8>,
}

/// The priority of a message
#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessagePriority {
    /// The message is urgent and should be delivered immediately.
    Urgent,

    /// The message is not important.
    Low,

    /// The message is important and should be delivered during the day if allowed.
    DaytimeOnly,
}

impl Default for MessagePriority {
    fn default() -> Self {
        Self::Low
    }
}

impl Display for MessagePriority {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Urgent => write!(f, "Urgent"),
            Self::Low => write!(f, "Low"),
            Self::DaytimeOnly => write!(f, "Daytime Only"),
        }
    }
}

/// A message to send
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    /// The title of the message.
    pub title: String,

    /// The message to send.
    pub body: String,
}

/// An audio command
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioCommand {
    /// The message to send.
    pub message: Option<Message>,

    /// The priority of this command.
    #[serde(default)]
    pub priority: MessagePriority,

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
    /// Decide if we should play this message
    #[cfg(feature = "chrono")]
    #[must_use]
    pub fn should_play(&self, now: chrono::DateTime<chrono::Local>, enabled: bool) -> bool {
        use chrono::Timelike;
        let day_hour = matches!(now.hour(), 7..=21);

        let priority = self.priority;
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

impl Display for AudioCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let message = self.message.as_ref();
        let music = &self.music;
        let play_list = music.as_ref().and_then(|music| music.play_list.as_ref());
        let music_stop = music.as_ref().and_then(|music| music.stop);
        let volume = &self.volume;
        let music_volume = volume.as_ref().and_then(|volume| volume.music);
        let message_volume = volume.as_ref().and_then(|volume| volume.message);
        let mut results = vec![];
        if let Some(volume) = message_volume {
            results.push(format!("set message volume {volume}"));
        }
        if let Some(message) = message {
            results.push(format!("say \"{}\"", message.body));
        }
        if let Some(music_stop) = music_stop {
            if music_stop {
                results.push("stop music".to_string());
            }
        }
        if let Some(volume) = music_volume {
            results.push(format!("set music volume {volume}"));
        }
        if let Some(play_list) = play_list {
            results.push(format!("play {play_list}"));
        }
        if results.is_empty() {
            write!(f, "unknown audio command")
        } else {
            write!(f, "{results}", results = results.join(", "))
        }
    }
}
