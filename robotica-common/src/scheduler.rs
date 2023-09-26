//! Scheduler struct shared between robotica-backend and robotica-frontend
use std::collections::HashSet;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    datetime::{DateTime, Duration},
    robotica::tasks::Task,
};

/// The status of the Mark.
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub enum MarkStatus {
    /// The tasks are to be cancelled.
    #[serde(rename = "cancelled")]
    Cancelled,

    /// The tasks are already done.
    #[serde(rename = "done")]
    Done,
}

/// A mark on a step.
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct Mark {
    /// The id of the step.
    pub id: String,

    /// The status of the Mark.
    pub status: MarkStatus,

    /// The start time of the Mark.
    pub start_time: DateTime<Utc>,

    /// The end time of the Mark.
    pub stop_time: DateTime<Utc>,
}

/// An error that can occur when parsing a mark.
#[derive(Error, Debug)]
pub enum MarkError {
    /// The Mark is invalid.
    #[error("Invalid mark {0}")]
    ParseError(#[from] serde_json::Error),

    /// UTF-8 error in Mark.
    #[error("Invalid UTF8")]
    Utf8Error(#[from] std::str::Utf8Error),
}

/// The schedule with all values completed.
///
/// Note this is not used in the backend, which has its own copy.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Sequence {
    /// The id of the step.
    pub id: String,

    /// The name of this sequence.
    // #[serde(skip)]
    // pub sequence_name: String,

    /// The conditions that must be true before this is scheduled.
    // #[serde(skip)]
    // if_cond: Option<Vec<Boolean<Context>>>,

    /// The required classifications for this step.
    // #[serde(skip)]
    // classifications: Option<HashSet<String>>,

    // The required options for this step.
    // #[serde(skip)]
    // options: Option<HashSet<String>>,

    /// If true this is considered the "zero time" for this sequence.
    // #[serde(skip)]
    // zero_time: bool,

    /// The start time of this step.
    pub required_time: DateTime<Utc>,

    /// The required duration of this step.
    #[serde(with = "crate::datetime::with_duration")]
    pub required_duration: Duration,

    /// The latest time this step can be completed.
    pub latest_time: DateTime<Utc>,

    /// The number of the repeat.
    pub repeat_number: usize,

    /// The tasks to execute.
    pub tasks: Vec<Task>,

    /// The mark for this task - for use by executor.
    pub mark: Option<Mark>,
}

// impl Ord for Sequence {
//     fn cmp(&self, other: &Self) -> std::cmp::Ordering {
//         self.required_time.cmp(&other.required_time)
//     }
// }

// impl PartialOrd for Sequence {
//     fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
//         self.required_time.partial_cmp(&other.required_time)
//     }
// }
// impl Eq for Sequence {}

// impl PartialEq for Sequence {
//     fn eq(&self, other: &Self) -> bool {
//         self.required_time == other.required_time
//     }
// }

/// The tags for yesterday, today, and tomorrow.
#[derive(Debug, Deserialize, Serialize)]
pub struct Tags {
    /// The tags for yesterday.
    pub yesterday: HashSet<String>,

    /// The tags for today.
    pub today: HashSet<String>,

    /// The tags for tomorrow.
    pub tomorrow: HashSet<String>,
}
