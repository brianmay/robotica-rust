pub mod filters;
pub mod sinks;
pub mod sources;

use std::{future::Future, time::Duration};

use log::*;
use tokio::{sync::mpsc::Sender, task::JoinHandle, time::timeout};

pub const PIPE_SIZE: usize = 10;

pub async fn send_and_wait<T>(tx: &Sender<T>, data: T) {
    let rc = timeout(Duration::from_secs(60), tx.send(data)).await;

    match rc {
        Ok(rc) => match rc {
            Ok(_) => {}
            Err(_) => panic!("send operation failed: receiver closed connection"),
        },
        Err(_) => panic!("send operation failed: timeout error"),
    };
}

pub async fn send_or_discard<T>(tx: &Sender<T>, data: T) {
    tx.try_send(data).unwrap_or_else(|err| match err {
        tokio::sync::mpsc::error::TrySendError::Full(_) => {
            error!("send operation failed: pipe is full");
        }
        tokio::sync::mpsc::error::TrySendError::Closed(_) => {
            panic!("send operation failed: receiver closed connection");
        }
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
