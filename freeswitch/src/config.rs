use envconfig::Envconfig;
use robotica_backend::services::mqtt;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::{freeswitch, phone_db};

#[derive(Envconfig)]
pub struct Environment {
    #[envconfig(from = "CONFIG_FILE")]
    pub config_file: PathBuf,

    #[envconfig(from = "SECRETS_FILE")]
    pub secrets_file: Option<PathBuf>,
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
            robotica_backend::serde::merge_yaml(config, secrets)?
        } else {
            config
        };

        let config: Config =
            serde_yml::from_value(config).map_err(|e| Error::Yaml(self.config_file.clone(), e))?;

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
    pub phone_db: phone_db::Config,
    pub freeswitch: freeswitch::Config,
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
    Merge(#[from] robotica_backend::serde::Error),
}
