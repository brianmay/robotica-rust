pub mod filters;
pub mod sinks;
pub mod sources;

use std::future::Future;

use log::*;
use tokio::{sync::mpsc::Sender, task::JoinHandle};

pub async fn send<T>(tx: &Sender<T>, data: T) {
    let a = tx.send(data).await;
    a.unwrap_or_else(|err| {
        panic!("send operation failed {err}");
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
