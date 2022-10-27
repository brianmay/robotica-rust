//! Common interfaces between robotica frontends and backends
use std::fmt::{Display, Formatter};

use serde::Deserialize;

/// The user on the backend
#[derive(Clone, Debug, Deserialize)]
pub struct User {
    /// The name of the user on the backend
    pub name: String,
}

impl Display for User {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
