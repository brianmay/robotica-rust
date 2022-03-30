pub mod filters;
pub mod sinks;
pub mod sources;

use anyhow::anyhow;
use anyhow::Result;
use log::*;
use std::{future::Future, time::Duration};
use tokio::{sync::mpsc::Sender, task::JoinHandle, time::timeout};

pub const PIPE_SIZE: usize = 10;

pub async fn send<T>(tx: &Sender<T>, duration: Duration, data: T) -> Result<()> {
    let rc = timeout(duration, tx.send(data)).await;

    match rc {
        Ok(rc) => match rc {
            Ok(_) => Ok(()),
            Err(_) => Err(anyhow!("send operation failed: receiver closed connection")),
        },
        Err(_) => Err(anyhow!("send operation failed: timeout error")),
    }
}

pub async fn send_or_panic<T>(tx: &Sender<T>, data: T) {
    send(tx, Duration::from_secs(30), data).await.unwrap();
}

pub async fn send_or_discard<T>(tx: &Sender<T>, data: T) {
    send(tx, Duration::from_secs(30), data)
        .await
        .unwrap_or_else(|err| {
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
