use crate::{amber, influxdb, tesla};
use envconfig::Envconfig;
use robotica_backend::{
    scheduling::executor,
    services::mqtt,
    services::{http, persistent_state},
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

fn load_file(filename: &Path) -> Result<serde_yaml::Value, Error> {
    let f = std::fs::File::open(filename).map_err(|e| Error::File(filename.to_path_buf(), e))?;
    let config: serde_yaml::Value =
        serde_yaml::from_reader(f).map_err(|e| Error::Yaml(filename.to_path_buf(), e))?;

    Ok(config)
}

impl Environment {
    pub fn config(&self) -> Result<Config, Error> {
        let config = load_file(&self.config_file)?;

        let config = if let Some(secrets_file) = &self.secrets_file {
            let secrets = load_file(secrets_file)?;
            robotica_backend::serde::merge_yaml(config, secrets)?
        } else {
            config
        };

        let config = {
            let mut env_config = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
            if let Some(database_url) = &self.database_url {
                env_config["database_url"] = serde_yaml::Value::String(database_url.clone());
            }
            robotica_backend::serde::merge_yaml(config, env_config)?
        };

        let mut config: Config =
            serde_yaml::from_value(config).map_err(|e| Error::Yaml(self.config_file.clone(), e))?;

        if let Some(static_path) = &self.static_path {
            config.http.static_path.clone_from(static_path);
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
    pub amber: amber::api::Config,
    pub http: http::Config,
    pub influxdb: influxdb::Config,
    pub executor: executor::ExtraConfig,
    pub persistent_state: persistent_state::Config,
    pub teslas: Vec<tesla::Config>,
    pub database_url: String,
}

/// An error loading the Config
#[derive(Error, Debug)]
pub enum Error {
    /// Error reading the file
    #[error("Error reading file {0}: {1}")]
    File(PathBuf, std::io::Error),

    /// Error reading the file
    #[error("Error parsing file {0}: {1}")]
    Yaml(PathBuf, serde_yaml::Error),

    /// Error merging the files
    #[error("Error merging files: {0}")]
    Merge(#[from] robotica_backend::serde::Error),
}
