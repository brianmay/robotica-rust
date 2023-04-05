//! Stream of data from a source.

use thiserror::Error;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::debug;
use tracing::error;

use crate::spawn;
use crate::PIPE_SIZE;

/// Incoming previous value and current value.
pub type StatefulData<T> = (Option<T>, T);

enum SendMessage<T> {
    Set(T),
}

enum ReceiveMessage<T> {
    Get(oneshot::Sender<Option<T>>),
    Subscribe(oneshot::Sender<(broadcast::Receiver<T>, Option<T>)>),
}

/// Send a value to an entity.
#[derive(Clone)]
pub struct Sender<T> {
    #[allow(dead_code)]
    name: String,
    tx: mpsc::Sender<SendMessage<T>>,
}

impl<T: Send> Sender<T> {
    /// Send data to the entity.
    // pub async fn send(&self, data: T) {
    //     let msg = SendMessage::Set(data);
    //     if let Err(err) = self.tx.send(msg).await {
    //         error!("send failed: {}", err);
    //     }
    // }

    /// Send data to the entity or fail if buffer is full.
    pub fn try_send(&self, data: T) {
        let msg = SendMessage::Set(data);
        if let Err(err) = self.tx.try_send(msg) {
            error!("send failed: {}", err);
        }
    }

    /// Is the remote end of the channel closed?
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Completes when the entity is closed.
    pub async fn closed(&self) {
        self.tx.closed().await;
    }
}

/// A `Receiver` that doesn't count as a reference to the entity.
pub struct WeakReceiver<T> {
    name: String,
    tx: mpsc::WeakSender<ReceiveMessage<T>>,
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
    name: String,
    tx: mpsc::Sender<ReceiveMessage<T>>,
}

impl<T: Send + Clone> Receiver<T> {
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
        (rx.await).map_or_else(
            |_| {
                error!("{}: get/await failed", self.name);
                None
            },
            |v| v,
        )
    }

    /// Subscribe to this entity.
    ///
    /// Returns an already closed subscription if the entity is closed.
    pub async fn subscribe(&self) -> Subscription<T> {
        let (tx, rx) = oneshot::channel();
        let msg = ReceiveMessage::Subscribe(tx);
        if let Err(err) = self.tx.send(msg).await {
            error!("{}: subscribe/send failed: {}", self.name, err);
            return Subscription::null(self.tx.clone());
        };
        if let Ok((rx, initial)) = rx.await {
            Subscription {
                rx,
                _tx: self.tx.clone(),
                initial,
            }
        } else {
            error!("{}: subscribe/await failed", self.name);
            Subscription::null(self.tx.clone())
        }
    }

    // /// Subscribe to this entity and translate the data.
    // pub async fn subscribe_into<U>(&self) -> Subscription<U>
    // where
    //     T: Send + 'static,
    //     U: TryInto<T> + Clone + Eq + Send + 'static,
    // {
    //     let s = self.try_into::<U>().await;
    //     s.subscribe().await
    // }

    /// Translate this receiver into a another type using a stateless receiver.
    #[must_use]
    pub fn translate_into_stateless<U>(self) -> Receiver<U>
    where
        T: Send + 'static,
        U: TryFrom<T> + Clone + Send + 'static,
        <U as TryFrom<T>>::Error: Send + std::error::Error,
    {
        let name = format!("{} (translate_into_stateless)", self.name);
        let (tx, rx) = create_stateless_entity(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                select! {
                    data = sub.recv() => {
                        let data = match data {
                            Ok(data) => data,
                            Err(err) => {
                                debug!("{}: translate_into_stateless({}): recv failed, exiting", name, err);
                                break;
                            }
                        };

                        match U::try_from(data) {
                            Ok(data) => {
                                tx.try_send(data);
                            }
                            Err(err) => {
                                error!("translate_into_stateless({}): parse failed: {}", name, err);
                            }
                        }
                    }

                    _ = tx.closed() => {
                        debug!("translate_into_stateless({}): source closed", name);
                        break;
                    }
                }
            }
        });

        rx
    }

    /// Translate this receiver into a another type using a stateful receiver.
    #[must_use]
    pub fn translate_into_stateful<U>(self) -> Receiver<StatefulData<U>>
    where
        T: Send + 'static,
        U: TryFrom<T> + Clone + Eq + Send + 'static,
        <U as TryFrom<T>>::Error: Send + std::error::Error,
    {
        let name = format!("{} (translate_into_stateful)", self.name);
        let (tx, rx) = create_stateful_entity(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                select! {
                    data = sub.recv() => {
                        let data = match data {
                            Ok(data) => data,
                            Err(err) => {
                                debug!("{}: translate_into_stateful({}): recv failed, exiting", name, err);
                                break;
                            }
                        };

                        match U::try_from(data) {
                            Ok(data) => {
                                tx.try_send(data);
                            }
                            Err(err) => {
                                error!("translate_into_stateful({}): parse failed: {}", name, err);
                            }
                        }
                    }

                    _ = tx.closed() => {
                        debug!("translate_into_stateful({}): source closed", name);
                        break;
                    }
                }
            }
        });

        rx
    }

    /// Map this receiver into a another type using a stateless receiver.
    #[must_use]
    pub fn map_into_stateless<U>(self, f: impl Fn(T) -> U + Send + 'static) -> Receiver<U>
    where
        T: Send + 'static,
        U: Clone + Send + 'static,
    {
        let name = format!("{} (map_into_stateless)", self.name);
        let (tx, rx) = create_stateless_entity(name);
        self.map(tx, f);
        rx
    }

    /// Map this receiver into a another type using a stateful receiver.
    #[must_use]
    pub fn map_into_stateful<U>(
        self,
        f: impl Fn(T) -> U + Send + 'static,
    ) -> Receiver<StatefulData<U>>
    where
        T: Send + 'static,
        U: Clone + Eq + Send + 'static,
    {
        let name = format!("{} (map_into_stateful)", self.name);
        let (tx, rx) = create_stateful_entity(name);
        self.map(tx, f);
        rx
    }

    /// Map this receiver into a another type using a stateless receiver.
    fn map<U>(self, tx: Sender<U>, f: impl Fn(T) -> U + Send + 'static)
    where
        T: Send + 'static,
        U: Clone + Send + 'static,
    {
        let name = format!("{} (map)", self.name);

        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                select! {
                    data = sub.recv() => {
                        let data = match data {
                            Ok(data) => data,
                            Err(err) => {
                                debug!("{}: map({}): recv failed, exiting", name, err);
                                break;
                            }
                        };

                        tx.try_send(f(data));
                    }

                    _ = tx.closed() => {
                        debug!("map({}): dest closed", name);
                        break;
                    }
                }
            }
        });
    }

    /// Map this receiver into a another type using a stateless receiver.
    #[must_use]
    pub fn filter_into_stateless(self, f: impl Fn(&T) -> bool + Send + 'static) -> Receiver<T>
    where
        T: Send + 'static,
    {
        let name = format!("{} (map_into_stateless)", self.name);
        let (tx, rx) = create_stateless_entity(name);
        self.filter(tx, f);
        rx
    }

    /// Map this receiver into a another type using a stateful receiver.
    #[must_use]
    pub fn filter_into_stateful(
        self,
        f: impl Fn(&T) -> bool + Send + 'static,
    ) -> Receiver<StatefulData<T>>
    where
        T: Send + Eq + 'static,
    {
        let name = format!("{} (map_into_stateful)", self.name);
        let (tx, rx) = create_stateful_entity(name);
        self.filter(tx, f);
        rx
    }

    /// Filter this receiver based on function result
    fn filter(self, tx: Sender<T>, f: impl Fn(&T) -> bool + Send + 'static)
    where
        T: Send + 'static,
    {
        let name = format!("{} (filter)", self.name);

        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                select! {
                    data = sub.recv() => {
                        let data = match data {
                            Ok(data) => data,
                            Err(err) => {
                                debug!("{}: map_into_stateless({}): recv failed, exiting", name, err);
                                break;
                            }
                        };

                        if f(&data) {
                            tx.try_send(data);
                        }
                    }

                    _ = tx.closed() => {
                        debug!("map_into_stateless({}): dest closed", name);
                        break;
                    }
                }
            }
        });
    }

    /// Run a function for every value received.
    pub fn for_each<F>(self, f: F)
    where
        F: Fn(T) + Send + 'static,
        T: Send + 'static,
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

impl<T: Send + Clone> Receiver<StatefulData<T>> {
    /// Get the most recent value from the entity.
    pub async fn get_current(&self) -> Option<T> {
        self.get().await.map(|(_prev, current)| current)
    }
}

/// Something went wrong in Receiver.
#[derive(Error, Debug)]
pub enum RecvError {
    /// The Pipe was closed.
    #[error("The pipe was closed")]
    Closed,
}

/// A subscription to receive data from an entity.
pub struct Subscription<T> {
    rx: broadcast::Receiver<T>,
    // We need to keep this to ensure connection stays alive.
    _tx: mpsc::Sender<ReceiveMessage<T>>,
    initial: Option<T>,
}

impl<T: Clone> Subscription<T> {
    /// Create a null subscription that is already closed.
    fn null(tx: mpsc::Sender<ReceiveMessage<T>>) -> Self {
        let (_tx, rx) = broadcast::channel(0);
        Self {
            rx,
            _tx: tx,
            initial: None,
        }
    }
}

impl<T: Send + Clone> Subscription<T> {
    /// Wait for the next value from the entity.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub async fn recv(&mut self) -> Result<T, RecvError> {
        let initial = self.initial.take();
        if let Some(initial) = initial {
            return Ok(initial);
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
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub fn try_recv(&mut self) -> Result<Option<T>, RecvError> {
        let initial = self.initial.take();
        if let Some(initial) = initial {
            return Ok(Some(initial));
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

impl<T: Send + Clone> Subscription<StatefulData<T>> {
    /// Wait for the next value from the entity.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub async fn recv_current(&mut self) -> Result<T, RecvError> {
        self.recv().await.map(|(_prev, current)| current)
    }

    /// Get the next value but don't wait for it. Returns `None` if there is no value.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub fn try_recv_current(&mut self) -> Result<Option<T>, RecvError> {
        self.try_recv()
            .map(|opt| opt.map(|(_prev, current)| current))
    }
}

/// Create a stateless entity that sends every message.
#[must_use]
pub fn create_stateless_entity<T: Clone + Send + 'static>(
    name: impl Into<String>,
) -> (Sender<T>, Receiver<T>) {
    let (send_tx, mut send_rx) = mpsc::channel::<SendMessage<T>>(PIPE_SIZE);
    let (receive_tx, receive_rx) = mpsc::channel::<ReceiveMessage<T>>(PIPE_SIZE);
    let (out_tx, out_rx) = broadcast::channel::<T>(PIPE_SIZE);

    drop(out_rx);

    let name = name.into();

    let sender = Sender {
        tx: send_tx,
        name: name.clone(),
    };
    let receiver = Receiver {
        tx: receive_tx,
        name: name.clone(),
    };

    spawn(async move {
        let name = name;
        let mut saved_data: Option<T> = None;
        let mut receive_rx = Some(receive_rx);

        loop {
            select! {
                Some(msg) = send_rx.recv() => {
                    match msg {
                        SendMessage::Set(data) => {
                            saved_data = Some(data.clone());
                            if let Err(_err) = out_tx.send(data) {
                                // It is not an error if there are no subscribers.
                                // debug!("create_stateless_entity({name}): send to broadcast failed: {err} (not an error)");
                            }
                        }
                    }
                }
                Some(msg) = try_receive(&mut receive_rx) => {
                    match msg {
                        Some(ReceiveMessage::Get(tx)) => {
                            if tx.send(saved_data.clone()).is_err() {
                                error!("create_stateless_entity{name}): get send failed");
                            };
                        }
                        Some(ReceiveMessage::Subscribe(tx)) => {
                            let rx = out_tx.subscribe();
                            if tx.send((rx, saved_data.clone())).is_err() {
                                error!("create_stateless_entity({name}): subscribe send failed");
                            };
                        }
                        None => {
                            debug!("create_stateless_entity({name}): command channel closed");
                            receive_rx = None;
                        }
                    }
                }
                else => {
                    debug!("create_stateless_entity({name}): all inputs closed");
                    break;
                }
            }

            if matches!((&receive_rx, out_tx.receiver_count()), (None, 0)) {
                debug!(
                    "create_stateless_entity({name}): receiver closed and all subscriptions closed"
                );
                break;
            }
        }
    });

    (sender, receiver)
}

/// Create a stateful entity that only produces messages when there is a change.
#[must_use]
pub fn create_stateful_entity<T: Clone + Eq + Send + 'static>(
    name: impl Into<String>,
) -> (Sender<T>, Receiver<StatefulData<T>>) {
    let (send_tx, mut send_rx) = mpsc::channel::<SendMessage<T>>(PIPE_SIZE);
    let (receive_tx, receive_rx) = mpsc::channel::<ReceiveMessage<StatefulData<T>>>(PIPE_SIZE);
    let (out_tx, out_rx) = broadcast::channel::<StatefulData<T>>(PIPE_SIZE);

    drop(out_rx);

    let name = name.into();

    let sender = Sender {
        tx: send_tx,
        name: name.clone(),
    };
    let receiver = Receiver {
        tx: receive_tx,
        name: name.clone(),
    };

    spawn(async move {
        let name = name;

        let mut prev_data: Option<T> = None;
        let mut saved_data: Option<T> = None;
        let mut receive_rx = Some(receive_rx);

        loop {
            select! {
                Some(msg) = send_rx.recv() => {
                    match msg {
                        SendMessage::Set(data) => {
                            let changed = saved_data.as_ref().map_or(true, |saved_data| data != *saved_data);
                            if changed {
                                prev_data = saved_data.clone();
                                saved_data = Some(data.clone());
                                if let Err(err) = out_tx.send((prev_data.clone(), data)) {
                                    // It is not an error if there are no subscribers.
                                    debug!("create_stateful_entity({name}): send to broadcast failed: {err} (not an error)");
                                }
                            };
                        }
                    }
                }
                Some(msg) = try_receive(&mut receive_rx) => {
                    match msg {
                        Some(ReceiveMessage::Get(tx)) => {
                            let data  = saved_data.clone().map(|data| (prev_data.clone(), data));
                            if tx.send(data).is_err() {
                                error!("create_stateful_entity({name}): get send failed");
                            };
                        }
                        Some(ReceiveMessage::Subscribe(tx)) => {
                            let data  = saved_data.clone().map(|data| (prev_data.clone(), data));
                            let rx = out_tx.subscribe();
                            if tx.send((rx, data)).is_err() {
                                error!("create_stateful_entity{name}): subscribe send failed");
                            };
                        }
                        None => {
                            debug!("create_stateful_entity({name}): command channel closed");
                            receive_rx = None;
                        }
                    }
                }
                else => {
                    debug!("create_stateful_entity({name}): all inputs closed");
                    break;
                }
            }

            if matches!((&receive_rx, out_tx.receiver_count()), (None, 0)) {
                debug!(
                    "create_stateful_entity({name}): receiver closed and all subscriptions closed"
                );
                break;
            }
        }
    });

    (sender, receiver)
}

async fn try_receive<T: Send>(rx: &mut Option<mpsc::Receiver<T>>) -> Option<Option<T>> {
    match rx {
        Some(rx) => Some(rx.recv().await),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[tokio::test]
    async fn test_stateless_entity() {
        let (tx, rx) = create_stateless_entity::<String>("test");
        tx.try_send("hello".to_string());
        let mut s = rx.subscribe().await;
        tx.try_send("goodbye".to_string());

        let current = s.recv().await.unwrap();
        assert_eq!("hello", current);

        let current = s.recv().await.unwrap();
        assert_eq!("goodbye", current);

        let result = rx.get().await;
        assert_eq!(Some("goodbye".to_string()), result);

        let result = s.try_recv().unwrap();
        assert!(result.is_none());

        let result = rx.get().await;
        assert_eq!(Some("goodbye".to_string()), result);

        drop(s);
        drop(rx);
        tx.closed().await;
    }

    #[tokio::test]
    async fn test_stateful_entity() {
        let (tx, rx) = create_stateful_entity::<String>("test");
        tx.try_send("hello".to_string());
        let mut s = rx.subscribe().await;
        tx.try_send("goodbye".to_string());

        let (prev, current) = s.recv().await.unwrap();
        assert_eq!(None, prev);
        assert_eq!("hello", current);

        let (prev, current) = s.recv().await.unwrap();
        assert_eq!("hello", prev.unwrap());
        assert_eq!("goodbye", current);

        let (prev, current) = rx.get().await.unwrap();
        assert_eq!("hello", prev.unwrap());
        assert_eq!("goodbye", current);

        let result = s.try_recv().unwrap();
        assert!(result.is_none());

        let (prev, current) = rx.get().await.unwrap();
        assert_eq!("hello", prev.unwrap());
        assert_eq!("goodbye", current);

        drop(s);
        drop(rx);
        tx.closed().await;
    }
}
