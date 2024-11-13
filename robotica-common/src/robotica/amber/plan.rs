//! Shared plan types

use std::fmt::Formatter;

use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::datetime::{datetime_to_string, time_delta};

/// A plan
#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct Plan {
    kw: f32,
    start_time: chrono::DateTime<Utc>,
    end_time: chrono::DateTime<Utc>,
}

impl Plan {
    /// Get the kw of the plan
    #[must_use]
    pub const fn get_kw(&self) -> f32 {
        self.kw
    }

    /// Get the start time of the plan
    #[must_use]
    pub const fn get_start_time(&self) -> chrono::DateTime<Utc> {
        self.start_time
    }

    /// Get the end time of the plan
    #[must_use]
    pub const fn get_end_time(&self) -> chrono::DateTime<Utc> {
        self.end_time
    }

    /// Get the time left in the plan
    #[must_use]
    pub fn get_time_left(&self, now: chrono::DateTime<Utc>) -> TimeDelta {
        if now < self.start_time {
            // hasn't started yet
            self.get_timedelta()
        } else if now < self.end_time {
            // started but not finished
            self.end_time - now
        } else {
            // finished
            TimeDelta::zero()
        }
    }

    /// Get the time delta of the plan
    #[must_use]
    pub fn get_timedelta(&self) -> TimeDelta {
        self.end_time - self.start_time
    }

    /// Check if the plan is current
    #[must_use]
    pub fn is_current(&self, dt: chrono::DateTime<Utc>) -> bool {
        self.start_time <= dt && self.end_time > dt
    }
}

impl std::fmt::Debug for Plan {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plan")
            .field("kw", &self.kw)
            .field("start_time", &datetime_to_string(self.start_time))
            .field("end_time", &datetime_to_string(self.end_time))
            .field("duration", &time_delta::to_string(self.get_timedelta()))
            .finish()
    }
}
