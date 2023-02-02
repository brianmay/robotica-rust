//! Audio player Service

use serde::{Deserialize, Serialize};

/// The current volume levels
#[derive(Serialize, Deserialize, Debug)]
pub struct VolumeState {
    /// The volume level for music
    pub music: u8,
    /// The volume level for messages
    pub message: u8,
}

impl Default for VolumeState {
    fn default() -> Self {
        Self {
            music: 100,
            message: 100,
        }
    }
}

/// The current state of the audio player
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct State {
    /// The current playlist
    pub play_list: Option<String>,
    /// The current message
    pub error: Option<String>,
    /// The current volume levels
    pub volume: VolumeState,
}
