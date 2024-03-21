//! Tesla mate API types

use serde::{Deserialize, Serialize};

/// The position of the car
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Location {
    /// The latitude of the car
    pub latitude: f64,

    /// The longitude of the car
    pub longitude: f64,
}
