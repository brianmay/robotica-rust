//! Occupancy-related types for presence tracking and occupancy sensors

use serde::{Deserialize, Serialize};

/// The resultant Presence Tracker that is reported
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct PresenceTrackerValue {
    /// The room name
    pub room: Option<String>,
    /// The distance
    pub distance: Option<f32>,
}

/// The occupancy state
#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum OccupiedState {
    /// The space is occupied
    Occupied,
    /// The space is vacant
    Vacant,
}

impl OccupiedState {
    /// Is the space occupied?
    #[must_use]
    pub const fn is_occupied(&self) -> bool {
        matches!(self, OccupiedState::Occupied)
    }

    /// Is the space vacant?
    #[must_use]
    pub const fn is_vacant(&self) -> bool {
        matches!(self, OccupiedState::Vacant)
    }
}
