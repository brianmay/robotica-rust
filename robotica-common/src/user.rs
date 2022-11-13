//! Struct for end user
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

/// An authenticated end user
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    /// The user's identifier
    pub sub: String,

    /// The name of the user
    pub name: String,

    /// The email of the user
    pub email: String,
}

impl Display for User {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
