//! Stateless receiver code.

use thiserror::Error;
use tokio::{
    select,
    sync::{broadcast, mpsc, oneshot},
};
use tracing::{debug, error};

use crate::{
    pipes::{stateful, stateless},
    spawn,
};

/// Something went wrong in Receiver.
#[derive(Error, Debug)]
pub enum RecvError {
    /// The Pipe was closed.
    #[error("The pipe was closed")]
    Closed,
}

type SubscribeMessage<T> = (broadcast::Receiver<T>, Option<T>);
pub(in crate::pipes) enum ReceiveMessage<T> {
    // Get(oneshot::Sender<Option<T>>),
    Subscribe(oneshot::Sender<SubscribeMessage<T>>),
}

/// A `Receiver` that doesn't count as a reference to the entity.
pub struct WeakReceiver<T> {
    name: String,
    pub(in crate::pipes) tx: mpsc::WeakSender<ReceiveMessage<T>>,
}

impl<T> WeakReceiver<T> {
    /// Try to convert to a `Receiver`
    #[must_use]
    pub fn upgrade(&self) -> Option<Receiver<T>> {
        self.tx.upgrade().map(|tx| Receiver {
            name: self.name.clone(),
            tx,
        })
    }
}

// impl<T> WeakReceiver<T>
// where
//     T: Send + Clone,
// {
//     /// Get a stateless receiver
//     #[must_use]
//     pub fn into_stateless(&self) -> stateless::WeakReceiver<T> {
//         stateless::WeakReceiver {
//             name: self.name.clone(),
//             tx: self.stateless_tx.clone(),
//         }
//     }

//     /// Turn this into a stateful receiver.
//     #[must_use]
//     pub fn into_stateful(&self) -> stateful::WeakReceiver<T> {
//         stateful::WeakReceiver {
//             name: self.name.clone(),
//             tx: self.stateful_tx.clone(),
//         }
//     }
// }

/// Receive a value from an entity
#[derive(Debug, Clone)]
pub struct Receiver<T> {
    pub(super) name: String,
    pub(in crate::pipes) tx: mpsc::Sender<ReceiveMessage<T>>,
}

impl<T> Receiver<T> {
    async fn subscribe(&self) -> Option<(broadcast::Receiver<T>, Option<T>)>
    where
        T: Send,
    {
        let (tx, rx) = oneshot::channel();
        let msg = ReceiveMessage::Subscribe(tx);
        if let Err(err) = self.tx.send(msg).await {
            error!("{}: subscribe/send failed: {}", self.name, err);
            return None;
        }
        rx.await.map_or_else(
            |_| {
                error!("{}: subscribe/await failed", self.name);
                None
            },
            |(rx, initial)| Some((rx, initial)),
        )
    }

    /// Try to convert to a `WeakReceiver`
    #[must_use]
    pub fn downgrade(&self) -> WeakReceiver<T> {
        WeakReceiver {
            name: self.name.clone(),
            tx: self.tx.downgrade(),
        }
    }
}

impl<T> Receiver<T>
where
    T: Send + Clone,
{
    /// Get a stateless receiver
    #[must_use]
    pub fn into_stateless(&self) -> stateless::Receiver<T>
    where
        T: 'static,
    {
        let name = format!("{} (into_stateless)", self.name);
        let (tx, rx) = stateless::create_pipe(&name);

        let clone_self = self.clone();

        spawn(async move {
            let Some((mut sub, _)) = clone_self.subscribe().await else {
                return;
            };

            loop {
                select! {
                    data = sub.recv() => {
                        let data = match data {
                            Ok(data) => data,
                            Err(err) => {
                                debug!("{name}: recv failed, exiting: {err}");
                                break;
                            }
                        };

                        tx.try_send(data);
                    }

                    () = tx.closed() => {
                        debug!("{name}: dest closed");
                        break;
                    }
                }
            }
        });
        rx
    }

    /// Turn this into a stateful receiver.
    #[must_use]
    pub fn into_stateful(&self) -> stateful::Receiver<T>
    where
        T: Eq + 'static,
    {
        let name = format!("{} (into_stateful)", self.name);
        let (tx, rx) = stateful::create_pipe(&name);

        let clone_self = self.clone();

        spawn(async move {
            // We need to keep tx to ensure connection is not closed if self is dropped.
            let Some((mut sub, initial)) = clone_self.subscribe().await else {
                return;
            };

            if let Some(initial) = initial {
                tx.try_send(initial);
            }

            loop {
                select! {
                    data = sub.recv() => {
                        let data = match data {
                            Ok(data) => data,
                            Err(err) => {
                                debug!("{name}: recv failed, exiting: {err}");
                                break;
                            }
                        };

                        tx.try_send(data);
                    }

                    () = tx.closed() => {
                        debug!("{name}: dest closed");
                        break;
                    }
                }
            }
        });
        rx
    }
}
