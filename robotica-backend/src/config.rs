use crate::{
    amber, car, influxdb,
    lights::{self, Scene},
    metrics, InitState,
};
use envconfig::Envconfig;
use robotica_common::{
    mqtt::Json,
    robotica::entities::Id,
    robotica::lights::{PowerColor, SceneName},
};
use robotica_tokio::{
    devices::lifx::LifxId,
    pipes::stateful,
    scheduling::executor,
    services::{http, mqtt, persistent_state},
};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Envconfig)]
pub struct Environment {
    #[envconfig(from = "CONFIG_FILE")]
    pub config_file: PathBuf,

    #[envconfig(from = "SECRETS_FILE")]
    pub secrets_file: Option<PathBuf>,

    #[envconfig(from = "STATIC_PATH")]
    pub static_path: Option<PathBuf>,

    #[envconfig(from = "DATABASE_URL")]
    pub database_url: Option<String>,
}

fn load_file(filename: &Path) -> Result<serde_yml::Value, Error> {
    let f = std::fs::File::open(filename).map_err(|e| Error::File(filename.to_path_buf(), e))?;
    let config: serde_yml::Value =
        serde_yml::from_reader(f).map_err(|e| Error::Yaml(filename.to_path_buf(), e))?;

    Ok(config)
}

impl Environment {
    pub fn config(&self) -> Result<Config, Error> {
        let config = load_file(&self.config_file)?;

        let config = if let Some(secrets_file) = &self.secrets_file {
            let secrets = load_file(secrets_file)?;
            robotica_tokio::serde::merge_yaml(config, secrets)?
        } else {
            config
        };

        let config = {
            let mut env_config = serde_yml::Value::Mapping(serde_yml::Mapping::new());
            if let Some(database_url) = &self.database_url {
                env_config["database_url"] = serde_yml::Value::String(database_url.clone());
            }
            robotica_tokio::serde::merge_yaml(config, env_config)?
        };

        let mut config: Config =
            serde_yml::from_value(config).map_err(|e| Error::Yaml(self.config_file.clone(), e))?;

        if let Some(static_path) = &self.static_path {
            if let Some(http_config) = &mut config.http {
                http_config.static_path.clone_from(static_path);
            }
        }

        Ok(config)
    }

    /// Load the environment from the environment variables.
    pub fn load() -> Result<Self, envconfig::Error> {
        Self::init_from_env()
    }
}

#[derive(Deserialize)]
pub struct Config {
    pub mqtt: mqtt::Config,
    pub amber: Option<amber::api::Config>,
    pub http: Option<http::Config>,
    pub influxdb: influxdb::Config,
    pub executor: Option<executor::Config>,
    pub persistent_state: persistent_state::Config,
    pub cars: Vec<car::Config>,
    pub database_url: String,
    pub logging: crate::logging::Config,
    pub lights: Vec<LightConfig>,
    pub strips: Vec<StripConfig>,
    pub lifx: Option<LifxConfig>,
    pub metrics: Vec<metrics::ConfigMetric>,
    pub hot_water: Option<HotWaterConfig>,
}

/// An error loading the Config
#[derive(Error, Debug)]
pub enum Error {
    /// Error reading the file
    #[error("Error reading file {0}: {1}")]
    File(PathBuf, std::io::Error),

    /// Error reading the file
    #[error("Error parsing file {0}: {1}")]
    Yaml(PathBuf, serde_yml::Error),

    /// Error merging the files
    #[error("Error merging files: {0}")]
    Merge(#[from] robotica_tokio::serde::Error),
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type", content = "value")]
pub enum LightSceneConfig {
    FixedColor(PowerColor),
    MqttFeed(String),
}

impl LightSceneConfig {
    pub fn get_scene(&self, state: &mut InitState, name: SceneName) -> Scene {
        let entity = match self {
            Self::FixedColor(pc) => stateful::static_pipe(pc.clone(), "FixedColor"),
            Self::MqttFeed(topic) => state
                .subscriptions
                .subscribe_into_stateful::<Json<PowerColor>>(topic)
                .map(|(_, Json(c))| c),
        };
        lights::Scene::new(entity, name)
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Deserialize)]
pub struct LightConfig {
    pub device: LightDeviceConfig,
    pub id: Id,
    #[serde(default)]
    pub scenes: std::collections::HashMap<SceneName, LightSceneConfig>,
    pub flash_color: PowerColor,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Deserialize)]
pub struct StripConfig {
    pub device: LightDeviceConfig,
    pub id: Id,
    pub number_of_lights: usize,
    pub splits: Vec<SplitLightConfig>,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum LightDeviceConfig {
    Lifx { lifx_id: LifxId },
    Debug { lifx_id: LifxId },
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Deserialize)]
pub struct SplitLightConfig {
    pub id: Id,
    #[serde(default)]
    pub scenes: std::collections::HashMap<SceneName, LightSceneConfig>,
    pub flash_color: PowerColor,
    pub begin: usize,
    pub number: usize,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Deserialize)]
pub struct LifxConfig {
    pub broadcast: String,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Deserialize)]
pub struct HotWaterConfig {
    pub id: Id,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    #[test]
    fn test_light_device_deserialize() {
        let json = json!({"type": "lifx", "lifx_id": 0x1234_5678_90ab_cdefu64});
        let device: LightDeviceConfig = serde_json::from_value(json).unwrap();
        assert_eq!(
            device,
            LightDeviceConfig::Lifx {
                lifx_id: LifxId::new(0x1234_5678_90ab_cdef)
            }
        );

        let json = json!({"type": "debug", "lifx_id": 0x1234_5678_90ab_cdefu64});
        let device: LightDeviceConfig = serde_json::from_value(json).unwrap();
        assert_eq!(
            device,
            LightDeviceConfig::Debug {
                lifx_id: LifxId::new(0x1234_5678_90ab_cdef)
            }
        );
    }
}
