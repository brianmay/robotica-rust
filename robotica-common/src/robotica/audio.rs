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

/// An audio command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCommand {
    /// The title of the message.
    pub title: Option<String>,

    /// The message to send.
    pub message: Option<String>,

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
    /// Create a new message only command
    #[must_use]
    pub fn new_message(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            message: Some(message.into()),
            sound: None,
            music: None,
            volume: None,
            pre_tasks: None,
            post_tasks: None,
        }
    }
}
