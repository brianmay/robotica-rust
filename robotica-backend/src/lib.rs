//! Provide functionality to process asynchronous streams of data for IOT devices.
#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::use_self)]

#[cfg(feature = "scheduler")]
#[macro_use]
extern crate lalrpop_util;

// extern crate robotica_backend_macros;

pub mod database;
pub mod devices;
pub mod pipes;
pub mod serde;
pub mod services;
pub mod sinks;
pub mod sources;

#[cfg(feature = "scheduler")]
pub mod scheduling;

use std::{env, future::Future};
use tokio::task::JoinHandle;
use tracing::{debug, error};

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
