use envconfig::Envconfig;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Envconfig)]
pub struct Environment {
    #[envconfig(from = "AMBER_TOKEN")]
    pub amber_token: String,

    #[envconfig(from = "AMBER_SITE_ID")]
    pub amber_site_id: String,

    #[envconfig(from = "INFLUXDB_URL")]
    pub influxdb_url: String,

    #[envconfig(from = "INFLUXDB_DATABASE")]
    pub influxdb_database: String,

    #[envconfig(from = "MQTT_USERNAME")]
    pub mqtt_username: String,

    #[envconfig(from = "MQTT_PASSWORD")]
    pub mqtt_password: String,

    #[envconfig(from = "MQTT_HOST")]
    pub mqtt_host: String,

    #[envconfig(from = "MQTT_PORT")]
    pub mqtt_port: u16,

    #[envconfig(from = "LIFE360_USERNAME")]
    pub life360_username: String,

    #[envconfig(from = "LIFE360_PASSWORD")]
    pub life360_password: String,

    #[envconfig(from = "OIDC_DISCOVERY_URL")]
    pub oidc_discovery_url: String,

    #[envconfig(from = "OIDC_CLIENT_ID")]
    pub oidc_client_id: String,

    #[envconfig(from = "OIDC_CLIENT_SECRET")]
    pub oidc_client_secret: String,

    #[envconfig(from = "OIDC_SCOPES")]
    pub oidc_scopes: String,

    #[envconfig(from = "CALENDAR_URL")]
    pub calendar_url: String,

    #[envconfig(from = "CONFIG_FILE")]
    pub config_file: PathBuf,

    #[envconfig(from = "SESSION_SECRET")]
    pub session_secret: String,
}

impl Environment {
    /// Load the environment from the environment variables.
    pub fn load() -> Result<Self, envconfig::Error> {
        Self::init_from_env()
    }
}

#[derive(Deserialize)]
pub struct Config {
    pub state_path: PathBuf,
    pub static_path: PathBuf,
    pub http_listener: String,
    pub hostname: String,
    pub root_url: reqwest::Url,
    pub debug: bool,
    pub classifications_file: PathBuf,
    pub schedule_file: PathBuf,
    pub sequences_file: PathBuf,
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
}

impl Config {
    /// Load the classifier config from the given path.
    ///
    /// # Errors
    ///
    /// If the file cannot be read or parsed.
    pub fn load(filename: &Path) -> Result<Self, Error> {
        let f =
            std::fs::File::open(filename).map_err(|e| Error::File(filename.to_path_buf(), e))?;

        let config: Self =
            serde_yaml::from_reader(f).map_err(|e| Error::Yaml(filename.to_path_buf(), e))?;

        Ok(config)
    }
}
