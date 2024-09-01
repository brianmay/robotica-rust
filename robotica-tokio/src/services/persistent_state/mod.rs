//! Save persistent state to disk
use std::{io::Write, marker::PhantomData, path::PathBuf};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

use crate::entities::Id;

/// Configuration for `PersistentState`.
#[derive(Deserialize)]
pub struct Config {
    /// Path to the directory where state should be saved.
    pub state_path: PathBuf,
}

/// Errors that can occur when using a `PersistentState`.
#[derive(Error, Debug)]
pub enum Error {
    /// An IO error occurred.
    #[error("IO error file {0}: {1}")]
    IoError(String, std::io::Error),

    /// An error occurred while serializing or deserializing JSON.
    #[error("JSON error file {0}: {1}")]
    JsonError(String, serde_json::Error),
}

/// This is used to save state.
pub struct PersistentStateDatabase {
    path: PathBuf,
}

impl PersistentStateDatabase {
    /// Create a new `PersistentState` instance.
    ///
    /// The `STATE_DIR` environment variable must be set to the path of the directory where state.
    ///
    /// # Errors
    ///
    /// This function will return an error if the directory does not exist and cannot be created.
    pub fn new(config: &Config) -> Result<PersistentStateDatabase, Error> {
        let path = &config.state_path;
        if !path.is_dir() {
            std::fs::create_dir_all(path)
                .map_err(|e| Error::IoError(path.to_string_lossy().to_string(), e))?;
        }

        Ok(PersistentStateDatabase { path: path.clone() })
    }

    /// Get a `PersistentState` instance for a given name.
    #[must_use]
    pub fn for_name<T>(&self, id: &Id, name: &str) -> PersistentStateRow<T>
    where
        T: Serialize + DeserializeOwned,
    {
        let id = id.as_str().replace('/', "_");
        let name = name.replace('/', "_");
        let name = format!("{name}.json");
        let path = self.path.join(id).join(name);
        PersistentStateRow::new(path)
    }
}

/// This is used to save state.
pub struct PersistentStateRow<T: Serialize + DeserializeOwned> {
    path: PathBuf,
    phantom: PhantomData<T>,
}

impl<T: Serialize + DeserializeOwned> PersistentStateRow<T> {
    const fn new(path: PathBuf) -> Self {
        PersistentStateRow {
            path,
            phantom: PhantomData,
        }
    }

    /// Save a value to disk.
    ///
    /// # Errors
    ///
    /// This function will return an error if the value cannot be serialized to JSON or if the file
    /// cannot be written.
    pub fn save(&self, value: &T) -> Result<(), Error> {
        let parent = &self.path.parent();
        if let Some(parent) = parent {
            if !parent.is_dir() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| Error::IoError(parent.to_string_lossy().to_string(), e))?;
            }
        }

        let tmp_file = self.path.with_extension("tmp");

        let file = std::fs::File::create(&tmp_file)
            .map_err(|e| Error::IoError(tmp_file.to_string_lossy().to_string(), e))?;

        let mut writer = std::io::BufWriter::new(file);
        serde_json::to_writer(&mut writer, value)
            .map_err(|e| Error::JsonError(tmp_file.to_string_lossy().to_string(), e))?;

        writer
            .flush()
            .map_err(|e| Error::IoError(tmp_file.to_string_lossy().to_string(), e))?;

        std::fs::rename(tmp_file, &self.path)
            .map_err(|e| Error::IoError(self.path.to_string_lossy().to_string(), e))?;

        Ok(())
    }

    /// Load a value from disk.
    ///
    /// # Errors
    ///
    /// This function will return an error if the file cannot be read or if the value cannot be
    /// deserialized from JSON.
    pub fn load(&self) -> Result<T, Error> {
        let file = std::fs::File::open(&self.path)
            .map_err(|e| Error::IoError(self.path.to_string_lossy().to_string(), e))?;
        let reader = std::io::BufReader::new(file);
        let value = serde_json::from_reader(reader)
            .map_err(|e| Error::JsonError(self.path.to_string_lossy().to_string(), e))?;
        Ok(value)
    }
}
