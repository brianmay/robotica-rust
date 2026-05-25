//! Indexed receiver code.
//!
//! The receiver shares its message types with the stateful receiver so that
//! [`into_stateful`] can return a [`stateful::Receiver`] backed by the same
//! channel — a true thin wrapper.

use async_trait::async_trait;
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tracing::{debug, error};

use crate::{
    pipes::{stateful, stateless, Subscriber, Subscription as SubscriptionTrait},
    spawn,
};

use stateful::receiver::Subscription;

// The generic pipe uses the same message types as the stateful pipe so that
// `into_stateful` can return a `stateful::Receiver` backed by the same channel.
pub(in crate::pipes) use stateful::receiver::ReceiveMessage;

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

/// Receive a value from an entity.
#[derive(Debug, Clone)]
pub struct Receiver<T> {
    pub(super) name: String,
    pub(in crate::pipes) tx: mpsc::Sender<ReceiveMessage<T>>,
}

#[async_trait]
impl<T> Subscriber<T> for Receiver<T>
where
    T: Send + Clone + 'static,
{
    type SubscriptionType = Subscription<T>;

    /// Subscribe to this entity.
    ///
    /// Returns an already closed subscription if the entity is closed.
    async fn subscribe(&self) -> Subscription<T> {
        let (tx, rx) = oneshot::channel();
        let msg = ReceiveMessage::Subscribe(tx);
        if let Err(err) = self.tx.send(msg).await {
            error!("{}: subscribe/send failed: {}", self.name, err);
            return Subscription::null(self.tx.clone());
        }
        rx.await.map_or_else(
            |_| {
                error!("{}: subscribe/await failed", self.name);
                Subscription::null(self.tx.clone())
            },
            |(rx, initial)| Subscription {
                rx,
                _tx: self.tx.clone(),
                initial,
            },
        )
    }
}

impl<T> Receiver<T> {
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
    /// Retrieve the most recent value from the entity.
    ///
    /// Returns `None` if the entity is closed.
    pub async fn get(&self) -> Option<T> {
        let (tx, rx) = oneshot::channel();
        let msg = ReceiveMessage::Get(tx);
        if let Err(err) = self.tx.send(msg).await {
            error!("{}: get/send failed: {}", self.name, err);
            return None;
        }
        rx.await.unwrap_or_else(|_| {
            error!("{}: get/await failed", self.name);
            None
        })
    }
}

impl<T> Receiver<T>
where
    T: Send + Clone,
{
    /// Get a stateless receiver that forwards every message (new-value only).
    #[must_use]
    pub fn into_stateless(&self) -> stateless::Receiver<T>
    where
        T: 'static,
    {
        let name = format!("{} (into_stateless)", self.name);
        let (tx, rx) = stateless::create_pipe(&name);

        let clone_self = self.clone();

        spawn(async move {
            let mut sub = clone_self.subscribe().await;

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
    ///
    /// Because the generic pipe shares message types with the stateful pipe,
    /// this is a thin wrapper — no additional actor is created.
    #[must_use]
    pub fn into_stateful(&self) -> stateful::Receiver<T> {
        stateful::Receiver {
            name: self.name.clone(),
            tx: self.tx.clone(),
        }
    }
}
