//! Scheduler struct shared between robotica-tokio and robotica-frontend
use std::{
    collections::HashSet,
    fmt::{Display, Formatter},
    ops::Deref,
    time::Duration,
};

use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{datetime::DateTime, robotica::tasks::Task};

/// The status of the Sequence.
#[derive(Deserialize, Serialize, Copy, Clone, Debug, Eq, PartialEq)]
pub enum Status {
    /// The task has not started.
    Pending,
    /// The task is in progress.
    InProgress,
    /// The tasks is completed.
    Completed,
    /// The tasks are to be cancelled.
    Cancelled,
}

impl Status {
    /// Returns true if the status is done.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        matches!(self, Status::Completed | Status::Cancelled)
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Pending => write!(f, "Pending"),
            Status::InProgress => write!(f, "In Progress"),
            Status::Completed => write!(f, "Completed"),
            Status::Cancelled => write!(f, "Cancelled"),
        }
    }
}

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

impl Display for MarkStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MarkStatus::Cancelled => write!(f, "Cancelled"),
            MarkStatus::Done => write!(f, "Done"),
        }
    }
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
    pub end_time: DateTime<Utc>,
}

impl Display for Mark {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Mark {} {} valid {} to {}",
            self.id, self.status, self.start_time, self.end_time
        )
    }
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

/// The importance of a Sequence
#[derive(
    Copy, Clone, Debug, Default, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum Importance {
    /// The sequence is low importance
    ///
    /// It should never be displayed on schedules
    Low,

    /// The sequence is medium importance
    ///
    /// It should be displayed on verbose schedules.
    #[default]
    Medium,

    /// The sequence is high importance
    ///
    /// It should be displayed on all schedules.
    High,
}

impl Display for Importance {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Importance::Low => write!(f, "Low"),
            Importance::Medium => write!(f, "Medium"),
            Importance::High => write!(f, "High"),
        }
    }
}

/// The schedule with all values completed.
///
/// Note this is not used in the backend, which has its own copy.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Sequence {
    /// The title of the sequence.
    pub title: String,

    /// The id of the sequence.
    pub id: String,

    /// The source schedule date of the sequence.
    pub schedule_date: NaiveDate,

    /// The importance of the sequence.
    pub importance: Importance,

    // The name of this sequence.
    // #[serde(skip)]
    // pub sequence_name: String,

    // The conditions that must be true before this is scheduled.
    // #[serde(skip)]
    // if_cond: Option<Vec<Boolean<Context>>>,

    // The required classifications for this step.
    // #[serde(skip)]
    // classifications: Option<HashSet<String>>,

    // The required options for this step.
    // #[serde(skip)]
    // options: Option<HashSet<String>>,

    // If true this is considered the "zero time" for this sequence.
    // #[serde(skip)]
    // zero_time: bool,
    //
    /// The start time of this step.
    pub start_time: DateTime<Utc>,

    /// The end time of this step.
    pub end_time: DateTime<Utc>,

    /// The required duration of this step.
    #[serde(with = "crate::datetime::with_duration")]
    pub duration: Duration,

    /// The latest time this step can be started.
    pub latest_time: DateTime<Utc>,

    /// The number of the repeat.
    pub repeat_number: usize,

    /// The tasks to execute.
    pub tasks: Vec<Task>,

    /// The status for this task - for use by executor.
    pub status: Option<Status>,

    /// The mark for this task - for use by executor.
    pub mark: Option<Mark>,
}

impl Sequence {
    /// Returns true if the sequence is done.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.status.is_some_and(|status| status.is_done())
    }
}

/// The complete set of tags for a particular day
#[derive(Debug, Deserialize, Serialize)]
pub struct TagsForDay {
    /// The scheduled day for this entry
    pub date: NaiveDate,

    /// The tags for this day
    pub tags: HashSet<String>,
}

/// The tags for yesterday, today, and tomorrow.
#[derive(Debug, Deserialize, Serialize)]
pub struct Tags(pub Vec<TagsForDay>);

impl Deref for Tags {
    type Target = Vec<TagsForDay>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
