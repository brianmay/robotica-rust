//! Stateless receiver code.

use super::{create_pipe, Sender};
use crate::pipes::{Subscriber, Subscription as SubscriptionTrait, PIPE_SIZE};
use crate::{pipes::RecvError, spawn};
use async_trait::async_trait;
use tokio::{
    select,
    sync::{broadcast, mpsc, oneshot},
};
use tracing::{debug, error};

type SubscribeMessage<T> = broadcast::Receiver<T>;

pub(in crate::pipes) enum ReceiveMessage<T> {
    Subscribe(oneshot::Sender<SubscribeMessage<T>>),
}

/// A `Receiver` that doesn't count as a reference to the entity.
pub struct WeakReceiver<T> {
    pub(in crate::pipes) name: String,
    pub(in crate::pipes) tx: mpsc::WeakSender<ReceiveMessage<T>>,
}

impl<T> WeakReceiver<T> {
    /// Try to convert to a `Receiver`.
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
    pub(in crate::pipes) name: String,
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
            return Subscription::null(self.tx.clone(), self.name.clone());
        }
        rx.await.map_or_else(
            |_| {
                error!("{}: subscribe/await failed", self.name);
                Subscription::null(self.tx.clone(), self.name.clone())
            },
            |rx| Subscription {
                name: self.name.clone(),
                rx,
                _tx: self.tx.clone(),
            },
        )
    }
}

impl<T> Receiver<T>
where
    T: Send + Clone,
{
    /// Translate this receiver into a another type using a stateless receiver.
    #[must_use]
    pub fn translate<U>(self) -> Receiver<U>
    where
        T: 'static,
        U: Clone + Send + TryFrom<T> + 'static,
        <U as TryFrom<T>>::Error: std::error::Error,
    {
        let name = format!("{} (translate)", self.name);
        let (tx, rx) = create_pipe(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

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

                        match U::try_from(data) {
                            Ok(data) => {
                                tx.try_send(data);
                            }
                            Err(err) => {
                                error!("{name}: parse failed: {err}");
                            }
                        }
                    }

                    () = tx.closed() => {
                        debug!("{name}: source closed");
                        break;
                    }
                }
            }
        });

        rx
    }

    /// Map this receiver into a another type using a any type of receiver.
    pub fn map<U>(self, f: impl Fn(T) -> U + Send + 'static) -> Receiver<U>
    where
        T: 'static,
        U: Clone + Send + 'static,
    {
        let name = format!("{} (map_into)", self.name);
        let (tx, rx) = create_pipe(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

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

                        tx.try_send(f(data));
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

    /// Filter this receiver based on function result
    #[must_use]
    pub fn filter(self, f: impl Fn(&T) -> bool + Send + 'static) -> Receiver<T>
    where
        T: 'static,
    {
        let name = format!("{} (filter_into_anything)", self.name);
        let (tx, rx) = create_pipe(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

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

                        if f(&data) {
                            tx.try_send(data);
                        }
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

    /// Run a function for every value received.
    pub fn for_each<F>(self, f: F)
    where
        F: Fn(T) + Send + 'static,
        T: 'static,
    {
        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                while let Ok(data) = sub.recv().await {
                    f(data);
                }
            }
        });
    }

    /// Run an async function for every value received.
    /// The next value will not be processed until the future completes.
    pub fn for_each_async<F, Fut>(self, f: F)
    where
        F: Fn(T) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
        T: 'static,
    {
        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                while let Ok(data) = sub.recv().await {
                    f(data).await;
                }
            }
        });
    }

    /// Send the data to another pipe.
    pub fn send_to(self, dest: &Sender<T>)
    where
        T: 'static,
    {
        let dest = dest.clone();
        self.for_each(move |data| {
            dest.try_send(data);
        });
    }

    /// Completes when the entity is closed.
    pub async fn closed(&self) {
        self.tx.closed().await;
    }

    /// Create a new `WeakReceiver` from this Receiver.
    #[must_use]
    pub fn downgrade(&self) -> WeakReceiver<T> {
        WeakReceiver {
            tx: self.tx.downgrade(),
            name: self.name.clone(),
        }
    }
}

/// A subscription to receive data from an entity.
pub struct Subscription<T> {
    name: String,
    rx: broadcast::Receiver<T>,
    // We need to keep this to ensure connection stays alive.
    _tx: mpsc::Sender<ReceiveMessage<T>>,
}

impl<T> Subscription<T>
where
    T: Clone,
{
    /// Create a null subscription that is already closed.
    fn null(tx: mpsc::Sender<ReceiveMessage<T>>, name: String) -> Self {
        let (_tx, rx) = broadcast::channel(1);
        Self { name, rx, _tx: tx }
    }
}

#[async_trait]
impl<T> SubscriptionTrait<T> for Subscription<T>
where
    T: Send + Clone,
{
    /// Wait for the next value from the entity.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    async fn recv(&mut self) -> Result<T, RecvError> {
        loop {
            match self.rx.recv().await {
                Ok(v) => return Ok(v),
                Err(err) => match err {
                    broadcast::error::RecvError::Closed => return Err(RecvError::Closed),
                    broadcast::error::RecvError::Lagged(skipped) => {
                        error!(
                            "{}: recv failed: The pipe was lagged, {skipped} messages skipped (capacity {PIPE_SIZE})",
                            self.name
                        );
                    }
                },
            }
        }
    }

    /// Get the next value but don't wait for it. Returns `None` if there is no value.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    fn try_recv(&mut self) -> Result<Option<T>, RecvError> {
        loop {
            match self.rx.try_recv() {
                Ok(v) => return Ok(Some(v)),
                Err(err) => match err {
                    broadcast::error::TryRecvError::Closed => {
                        return Err(RecvError::Closed);
                    }
                    broadcast::error::TryRecvError::Empty => return Ok(None),
                    broadcast::error::TryRecvError::Lagged(skipped) => {
                        error!(
                            "{}: try_recv failed: The pipe was lagged, {skipped} messages skipped (capacity {PIPE_SIZE})",
                            self.name
                        );
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[tokio::test]
    async fn test_null_subscription() {
        let (tx, _rx) = mpsc::channel::<ReceiveMessage<u8>>(1);
        let mut sub = super::Subscription::null(tx, "test null subscription".into());
        assert!(sub.try_recv().is_err());
        assert!(sub.recv().await.is_err());
    }

    #[tokio::test]
    async fn test_recv_after_lag_returns_oldest_buffered_value() {
        // tokio rounds the broadcast capacity up to a power of two.
        const EFFECTIVE_CAPACITY: usize = PIPE_SIZE.next_power_of_two();
        const SENT: usize = EFFECTIVE_CAPACITY + 10;
        const SKIPPED: usize = 10;
        const CHUNK: usize = 20;

        let (tx, rx) = create_pipe("test lagged subscription");
        let mut victim = rx.subscribe().await;
        let mut monitor = rx.subscribe().await;

        // Flood the pipe in chunks small enough for the pipe's bounded
        // inbound channel, pausing between chunks so the task can
        // broadcast.
        for i in 0..SENT {
            tx.try_send(i);
            if i % CHUNK == CHUNK - 1 {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        }

        // Wait until the last value has been broadcast.
        loop {
            if monitor.recv().await.unwrap() == SENT - 1 {
                break;
            }
        }

        // The subscriber fell more than a full buffer behind: the first
        // SKIPPED values were dropped and recv resumes from the oldest
        // buffered value.
        let value = victim.recv().await.unwrap();
        assert_eq!(value, SKIPPED);
    }
}
