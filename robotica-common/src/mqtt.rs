//! Common Mqtt stuff
use serde::{Deserialize, Serialize};

/// The `QoS` level for a MQTT message.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Eq, PartialEq)]
pub enum QoS {
    /// At most once
    AtMostOnce,

    /// At least once
    AtLeastOnce,

    /// Exactly once
    ExactlyOnce,
}
