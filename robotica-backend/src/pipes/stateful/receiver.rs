//! Stateless receiver code.

use super::{create_pipe, Sender};
use crate::pipes::{Subscriber, Subscription as SubscriptionTrait};
use crate::{pipes::RecvError, spawn};
use async_trait::async_trait;
use tokio::{
    select,
    sync::{broadcast, mpsc, oneshot},
};
use tracing::{debug, error};

/// Old and new value.
pub type OldNewType<T> = (Option<T>, T);

type SubscribeMessage<T> = (broadcast::Receiver<OldNewType<T>>, Option<T>);

pub(in crate::pipes) enum ReceiveMessage<T> {
    Get(oneshot::Sender<Option<T>>),
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
            return Subscription::null(self.tx.clone());
        };
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
        };
        rx.await.unwrap_or_else(|_| {
            error!("{}: get/await failed", self.name);
            None
        })
    }

    /// Translate this receiver into a another type using a stateless receiver.
    #[must_use]
    pub fn translate<U>(self) -> Receiver<U>
    where
        T: 'static,
        U: PartialEq + Clone + Send + TryFrom<T> + 'static,
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
    pub fn map<U>(self, f: impl Fn(OldNewType<T>) -> U + Send + 'static) -> Receiver<U>
    where
        T: 'static,
        U: PartialEq + Clone + Send + 'static,
    {
        let name = format!("{} (map_into)", self.name);
        let (tx, rx) = create_pipe(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                select! {
                    data = sub.recv_old_new() => {
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
    pub fn filter(self, f: impl Fn(&OldNewType<T>) -> bool + Send + 'static) -> Receiver<T>
    where
        T: Eq + 'static,
    {
        let name = format!("{} (filter_into_anything)", self.name);
        let (tx, rx) = create_pipe(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                select! {
                    data = sub.recv_old_new() => {
                        let data = match data {
                            Ok(data) => data,
                            Err(err) => {
                                debug!("{name}: recv failed, exiting: {err}");
                                break;
                            }
                        };

                        if f(&data) {
                            let (_, new) = data;
                            tx.try_send(new);
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
        F: Fn(OldNewType<T>) + Send + 'static,
        T: 'static,
    {
        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                while let Ok(data) = sub.recv_old_new().await {
                    f(data);
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
        self.for_each(move |(_, data)| {
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
            name: self.name.clone(),
            tx: self.tx.downgrade(),
        }
    }
}

/// A subscription to receive data from an entity.
pub struct Subscription<T> {
    rx: broadcast::Receiver<OldNewType<T>>,
    // We need to keep this to ensure connection stays alive.
    _tx: mpsc::Sender<ReceiveMessage<T>>,
    initial: Option<T>,
}

impl<T> Subscription<T>
where
    T: Clone,
{
    /// Create a null subscription that is already closed.
    fn null(tx: mpsc::Sender<ReceiveMessage<T>>) -> Self {
        let (_tx, rx) = broadcast::channel(1);
        Self {
            rx,
            _tx: tx,
            initial: None,
        }
    }
}

impl<T> Subscription<T>
where
    T: Send + Clone,
{
    /// Wait for the next value from the entity.
    ///
    /// This will return (old value, new value)
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub async fn recv_old_new(&mut self) -> Result<OldNewType<T>, RecvError> {
        let initial = self.initial.take();
        if let Some(initial) = initial {
            return Ok((None, initial));
        }
        loop {
            match self.rx.recv().await {
                Ok(v) => return Ok(v),
                Err(err) => match err {
                    broadcast::error::RecvError::Closed => return Err(RecvError::Closed),
                    broadcast::error::RecvError::Lagged(_) => {
                        error!("recv failed: The pipe was lagged");
                    }
                },
            }
        }
    }

    /// Get the next value but don't wait for it. Returns `None` if there is no value.
    ///
    /// This will return (old value, new value)
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub fn try_recv_old_new(&mut self) -> Result<Option<OldNewType<T>>, RecvError> {
        let initial = self.initial.take();
        if let Some(initial) = initial {
            return Ok(Some((None, initial)));
        }
        loop {
            match self.rx.try_recv() {
                Ok(v) => return Ok(Some(v)),
                Err(err) => match err {
                    broadcast::error::TryRecvError::Closed => {
                        return Err(RecvError::Closed);
                    }
                    broadcast::error::TryRecvError::Empty => return Ok(None),
                    broadcast::error::TryRecvError::Lagged(_) => {
                        error!("try_recv failed: The pipe was lagged");
                    }
                },
            }
        }
    }
}

#[async_trait]
impl<T> SubscriptionTrait<T> for Subscription<T>
where
    T: Send + Clone,
{
    /// Wait for the next value from the entity.
    ///
    /// This will return the new value.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    async fn recv(&mut self) -> Result<T, RecvError> {
        match self.recv_old_new().await {
            Ok((_, v)) => Ok(v),
            Err(err) => Err(err),
        }
    }

    /// Get the next value but don't wait for it. Returns `None` if there is no value.
    ///
    /// This will return the new value.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    fn try_recv(&mut self) -> Result<Option<T>, RecvError> {
        match self.try_recv_old_new() {
            Ok(Some((_, v))) => Ok(Some(v)),
            Ok(None) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_null_subscription() {
        let (tx, _rx) = mpsc::channel::<ReceiveMessage<u8>>(1);
        let mut sub = super::Subscription::null(tx);
        assert!(sub.try_recv().is_err());
        assert!(sub.try_recv_old_new().is_err());
        assert!(sub.recv().await.is_err());
        assert!(sub.recv_old_new().await.is_err());
    }
}
