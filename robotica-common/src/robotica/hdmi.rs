//! Messages for Robotica HDMI matrix
use serde::{Deserialize, Serialize};

/// A command to send to a HDMI matrix.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HdmiCommand {
    /// The input to switch to.
    pub input: u8,

    /// The output to switch.
    pub output: u8,
}
