//! Struct for end user
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

/// An authenticated end user
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    /// The name of the user
    pub name: String,
}

impl Display for User {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
