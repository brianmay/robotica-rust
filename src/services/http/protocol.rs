use std::fmt::{Display, Formatter};

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(super) struct User {
    pub(super) name: String,
}

impl Display for User {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
