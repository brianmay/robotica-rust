#![warn(missing_docs)]
//! Provide functionality to process asynchronous streams of data for IOT devices.

pub mod entities;
pub mod sinks;
pub mod sources;

use log::*;
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
