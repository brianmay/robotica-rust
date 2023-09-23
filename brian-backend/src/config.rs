use std::path::{PathBuf, Path};

use robotica_backend::{EnvironmentOsError, get_env_os, scheduling::executor::ExtraConfig};
use serde::Deserialize;
use thiserror::Error;

#[derive(Deserialize)]
pub struct Config {
    pub executor: ExtraConfig,
}


/// An error loading the Config
#[derive(Error, Debug)]
pub enum Error {
    /// Environment variable not set
    #[error("{0}")]
    Environment(#[from] EnvironmentOsError),

    /// Error reading the file
    #[error("Error reading file {0}: {1}")]
    File(PathBuf, std::io::Error),

    /// Error reading the file
    #[error("Error parsing file {0}: {1}")]
    Yaml(PathBuf, serde_yaml::Error),
}

/// Load the classifier config from the given path.
///
/// # Errors
///
/// If the file cannot be read or parsed.
pub fn load(filename: &Path) -> Result<Config, Error> {
    let f = std::fs::File::open(filename)
        .map_err(|e| Error::File(filename.to_path_buf(), e))?;

    let config: Config = serde_yaml::from_reader(f)
        .map_err(|e| Error::Yaml(filename.to_path_buf(), e))?;

    Ok(config)
}

/// Load the classification config from the environment variable `CLASSIFICATIONS_FILE`.
///
/// # Errors
///
/// Returns an error if the environment variable `CLASSIFICATIONS_FILE` is not set or if the file cannot be read.
pub fn load_config_from_default_file() -> Result<Config, Error> {
    let env_name = "CONFIG_FILE";
    let filename = get_env_os(env_name)?;
    load(Path::new(&filename))
}