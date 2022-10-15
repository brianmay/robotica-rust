#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
// #![deny(clippy::unwrap_used)]
// #![deny(clippy::expect_used)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::use_self)]
//! Provide functionality to process asynchronous streams of data for IOT devices.

#[macro_use]
extern crate lalrpop_util;

// extern crate robotica_node_rust_macros;

pub mod devices;
pub mod entities;
pub mod scheduling;
pub mod sinks;
pub mod sources;

use log::error;
use std::{env, future::Future};
use tokio::task::JoinHandle;

/// Size of all pipes.
pub const PIPE_SIZE: usize = 10;

/// Spawn a task and automatically monitor its execution.
pub fn spawn<T>(future: T) -> JoinHandle<T::Output>
where
    T: Future + Send + 'static,
    T::Output: Send + 'static,
{
    let task = tokio::spawn(future);

    tokio::spawn(async move {
        let rc = task.await;

        match rc {
            Ok(_rc) => error!("The thread terminated"),
            Err(err) => error!("The thread aborted with error: {err}"),
        };

        std::process::exit(1);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_debug_mode() {
        assert!(is_debug_mode());
    }
}
