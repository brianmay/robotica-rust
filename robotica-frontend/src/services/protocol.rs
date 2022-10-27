//! Common interfaces between robotica frontends and backends
use std::fmt::{Display, Formatter};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct User {
    pub(super) name: String,
}

impl Display for User {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
