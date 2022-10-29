//! Common type manipulation stuff that is not shared with frontend
use thiserror::Error;

use robotica_common::scheduler::Mark;

use crate::services::mqtt::Message;

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

impl TryFrom<Message> for Mark {
    type Error = MarkError;

    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        let payload: String = msg.payload_into_string()?;
        let mark: Mark = serde_json::from_str(&payload)?;
        Ok(mark)
    }
}
