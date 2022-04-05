pub mod filters;
pub mod sinks;
pub mod sources;

use anyhow::anyhow;
use anyhow::Result;
use log::*;
use std::future::Future;
use tokio::{sync::broadcast, task::JoinHandle};

pub const PIPE_SIZE: usize = 10;

pub fn send<T>(tx: &broadcast::Sender<T>, data: T) -> Result<()> {
    let rc = tx.send(data);

    match rc {
        Ok(_) => Ok(()),
        Err(err) => Err(anyhow!("send operation failed: {err}")),
    }
}

pub fn send_or_discard<T>(tx: &broadcast::Sender<T>, data: T) {
    send(tx, data).unwrap_or_else(|err| {
        error!("{}", err);
    });
}

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

#[derive(Clone)]
pub struct Pipe<T>((), broadcast::Sender<T>);

impl<T: Clone> Pipe<T> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let (out_tx, _out_rx) = broadcast::channel(PIPE_SIZE);
        Self((), out_tx)
    }

    pub fn get_tx(&self) -> broadcast::Sender<T> {
        self.1.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.1.subscribe()
    }

    pub fn to_rx_pipe(&self) -> RxPipe<T> {
        RxPipe(self.1.clone())
    }

    pub fn to_tx_pipe(&self) -> TxPipe<T> {
        TxPipe(self.1.clone())
    }
}

#[derive(Clone)]
pub struct RxPipe<T>(broadcast::Sender<T>);

impl<T: Clone> RxPipe<T> {
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.0.subscribe()
    }
}

#[derive(Clone)]
pub struct TxPipe<T>(broadcast::Sender<T>);

impl<T: Clone> TxPipe<T> {
    pub fn get_tx(&self) -> broadcast::Sender<T> {
        self.0.clone()
    }
}
