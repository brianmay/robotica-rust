//! Provide functionality to process asynchronous streams of data for IOT devices.
#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
// #![deny(clippy::unwrap_used)]
// #![deny(clippy::expect_used)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::use_self)]

#[macro_use]
extern crate lalrpop_util;

// extern crate robotica_rust_macros;

pub mod devices;
pub mod entities;
pub mod scheduling;
pub mod services;
pub mod sinks;
pub mod sources;

use log::{debug, error};
use std::{env, future::Future};
use thiserror::Error;
use tokio::task::JoinHandle;

/// Size of all pipes.
pub const PIPE_SIZE: usize = 10;

/// Spawn a task and automatically monitor its execution.
pub fn spawn<T>(future: T) -> JoinHandle<()>
where
    T: Future + Send + 'static,
    T::Output: Send + 'static,
{
    let task = tokio::spawn(future);

    tokio::spawn(async move {
        let rc = task.await;

        match rc {
            Ok(_rc) => {
                debug!("The thread terminated normally");
            }
            Err(err) => {
                error!("The thread aborted with error: {err}");
                std::process::exit(1);
            }
        };
    })
}

/// Is this application being run in debug mode?
///
/// For unit tests (not integration tests), this is always true.
///
/// If the environment variable `ROBOTICA_DEBUG` is set to true, then this is true.
///
/// If being built in dev mode it will be true.
///
/// Otherwise, it is false.
///
/// Note integration tests will set `ROBOTICA_DEBUG` to false, so that they can test the
/// production code.
///
#[must_use]
pub fn is_debug_mode() -> bool {
    if cfg!(test) {
        return true;
    }

    if let Ok(value) = env::var("ROBOTICA_DEBUG") {
        return value.to_lowercase() == "true";
    }

    if cfg!(debug_assertions) {
        return true;
    }

    false
}

/// An error that can occur when getting and environment variable.
#[derive(Error, Debug)]
pub enum EnvironmentError {
    /// The environment variable is not set.
    #[error("The environment variable {0} is not set")]
    NotSet(String),

    /// The environment variable is not valid UTF-8.
    #[error("The environment variable {0} is not valid UTF-8")]
    NotUtf8(String),
}

/// Get environment variable and return usable error.
///
/// # Errors
///
/// If the environment variable is not set, or is not valid UTF-8, then an error is returned.
pub fn get_env(name: &str) -> Result<String, EnvironmentError> {
    match env::var(name) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) => Err(EnvironmentError::NotSet(name.to_string())),
        Err(env::VarError::NotUnicode(_)) => Err(EnvironmentError::NotUtf8(name.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_debug_mode() {
        assert!(is_debug_mode());
    }
}
