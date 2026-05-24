use robotica_common::robotica::entities::Id;
use robotica_common::robotica::message::Audience;
use serde::Deserialize;

use crate::{open_epaper_link, tesla};

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub id: Id,
    pub name: String,
    pub oel_display: Option<open_epaper_link::Config>,
    pub audience: AudienceConfig,

    /// Extra padding in metres added to zone boundaries when testing arrival.
    /// Defaults to `0.0` (exact boundary) — appropriate for high-accuracy GPS.
    #[serde(default = "Config::default_arrival_radius_m")]
    pub arrival_radius_m: f64,

    /// Extra padding in metres for the exit hysteresis test.
    /// Defaults to `10.0` metres — prevents flapping near zone edges.
    #[serde(default = "Config::default_exit_radius_m")]
    pub exit_radius_m: f64,

    #[serde(flatten)]
    pub make: MakeConfig,
}

impl Config {
    const fn default_arrival_radius_m() -> f64 {
        0.0
    }

    const fn default_exit_radius_m() -> f64 {
        10.0
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "make")]
pub enum MakeConfig {
    Tesla(tesla::Config),
    Unknown,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AudienceConfig {
    pub errors: Audience,
    pub locations: Audience,
    pub doors: Audience,
    pub charging: Audience,
    pub private: Audience,
}
