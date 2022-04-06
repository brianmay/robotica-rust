#![warn(missing_docs)]
//! Provide functionality to process asynchronous streams of data for IOT devices.

pub mod filters;
pub mod sinks;
pub mod sources;

use anyhow::anyhow;
use anyhow::Result;
use log::*;
use std::future::Future;
use tokio::sync::broadcast::error::RecvError;
use tokio::{sync::broadcast, task::JoinHandle};

const PIPE_SIZE: usize = 10;

/// Send a value to a broadcast channel, but use error value that does not contain the value.
pub fn send<T>(tx: &broadcast::Sender<T>, data: T) -> Result<()> {
    let rc = tx.send(data);

    match rc {
        Ok(_) => Ok(()),
        Err(err) => Err(anyhow!("send failed: {err}")),
    }
}

/// Send a value to a broadcast channel and log any errors.
pub fn send_or_log<T>(tx: &broadcast::Sender<T>, data: T) {
    send(tx, data).unwrap_or_else(|err| {
        error!("{}", err);
    });
}

/// Receive a value from a broadcast channel and log errors
pub async fn recv<T: Clone>(rx: &mut broadcast::Receiver<T>) -> Result<T, RecvError> {
    loop {
        match rx.recv().await {
            Ok(v) => break Ok(v),
            Err(err) => match err {
                RecvError::Closed => {
                    error!("The pipe was closed");
                    break Err(RecvError::Closed);
                }
                RecvError::Lagged(_) => error!("recv failed: The pipe was lagged"),
            },
        }
    }
}

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

/// A pipe is a channel that can be used to send and receive data to multiple senders/receivers.
pub struct Pipe<T>(broadcast::Sender<T>, Option<broadcast::Receiver<T>>);

impl<T: Clone> Pipe<T> {
    #[allow(clippy::new_without_default)]
    /// Create a new pipe.
    pub fn new() -> Self {
        let (out_tx, out_rx) = broadcast::channel(PIPE_SIZE);
        Self(out_tx, Some(out_rx))
    }

    /// Get the underlying sender for the pipe.
    pub fn get_tx(&self) -> broadcast::Sender<T> {
        self.0.clone()
    }

    /// Convert the pipe to an [RxPipe].
    pub fn to_rx_pipe(&self) -> RxPipe<T> {
        RxPipe(self.0.clone(), self.0.subscribe())
    }

    /// Convert the pipe to an [TxPipe].
    pub fn to_tx_pipe(&self) -> TxPipe<T> {
        // self.1 is dropped here
        TxPipe(self.0.clone())
    }
}

/// A pipe that can only be used to receive data.
///
/// Internally this keeps a copy of the Sender, which makes it possible to clone this object.
pub struct RxPipe<T>(broadcast::Sender<T>, broadcast::Receiver<T>);

impl<T: Clone> RxPipe<T> {
    fn new_from_sender(sender: broadcast::Sender<T>) -> Self {
        let rx = sender.subscribe();
        Self(sender, rx)
    }

    /// Get the underlying receiver for the pipe.
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.0.subscribe()
    }
}

impl<T> Clone for RxPipe<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.0.subscribe())
    }
}

/// A pipe that can only be used to send data.
#[derive(Clone)]
pub struct TxPipe<T>(broadcast::Sender<T>);

impl<T: Clone> TxPipe<T> {
    /// Get the underlying sender for the pipe.
    pub fn get_tx(&self) -> broadcast::Sender<T> {
        self.0.clone()
    }
}
