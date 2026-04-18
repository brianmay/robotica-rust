//! Hot water request and state

use serde::Deserialize;

use super::combined;

/// Hot water request
#[derive(Copy, Clone, Eq, PartialEq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    /// Heat the water
    Heat,
    /// Do not heat the water
    DoNotHeat,
}

impl std::fmt::Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Request::Heat => write!(f, "heat the water"),
            Request::DoNotHeat => write!(f, "do not heat the water"),
        }
    }
}

/// Hot water state
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct State {
    /// The combined state
    #[serde(flatten)]
    pub combined: combined::State<Request>,
}

impl State {
    /// Get the result of the hot water request
    #[must_use]
    pub const fn get_result(&self) -> &Request {
        self.combined.get_result()
    }
}
