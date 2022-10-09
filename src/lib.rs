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

pub mod entities;
pub mod scheduling;
pub mod sinks;
pub mod sources;

use log::error;
use std::future::Future;
use tokio::task::JoinHandle;

const PIPE_SIZE: usize = 10;

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
