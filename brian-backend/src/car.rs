use robotica_common::robotica::message::Audience;
use robotica_tokio::entities::Id;
use serde::Deserialize;

use crate::{open_epaper_link, tesla};

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub id: Id,
    pub name: String,
    pub amber_display: Option<open_epaper_link::Config>,
    pub audience: AudienceConfig,

    #[serde(flatten)]
    pub make: MakeConfig,
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
