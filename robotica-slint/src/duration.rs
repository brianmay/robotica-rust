//! Duration specific functions.
use std::time::Duration;

/// Convert a duration into a formatted string.
pub fn to_string(duration: &Duration) -> String {
    let seconds = duration.as_secs() % 60;
    let minutes = (duration.as_secs() / 60) % 60;
    let hours = (duration.as_secs() / 60) / 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}
