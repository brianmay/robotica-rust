//! Messages for Robotica HDMI matrix
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

/// A command to send to a HDMI matrix.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HdmiCommand {
    /// The input to switch to.
    pub input: u8,

    /// The output to switch.
    pub output: u8,
}

impl Display for HdmiCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let input = self.input;
        let output = self.output;
        write!(f, "HDMI #{input} -> #{output}")
    }
}
