//! Stream of data from a source.

use log::debug;
use log::error;
use thiserror::Error;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

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
}

/// Receive a value from an entity.
#[derive(Clone)]
pub struct Receiver<T> {
    name: String,
    tx: mpsc::Sender<ReceiveMessage<T>>,
}

impl<T: Send + Clone> Receiver<T> {
    /// Retrieve the most recent value from the entity.
    ///
    /// # Panics
    ///
    /// Panics if the receiver is disconnected.
    pub async fn get(&self) -> Option<T> {
        let (tx, rx) = oneshot::channel();
        let msg = ReceiveMessage::Get(tx);
        if let Err(err) = self.tx.send(msg).await {
            error!("{}: get/send failed: {}", self.name, err);
            panic!("get failed");
        };
        if let Ok(v) = rx.await {
            v
        } else {
            error!("{}: get/await failed", self.name);
            panic!("get failed");
        }
    }

    /// Subscribe to this entity.
    ///
    /// # Panics
    ///
    /// Panics if the receiver is disconnected.
    pub async fn subscribe(&self) -> Subscription<T> {
        let (tx, rx) = oneshot::channel();
        let msg = ReceiveMessage::Subscribe(tx);
        if let Err(err) = self.tx.send(msg).await {
            error!("{}: get/send failed: {}", self.name, err);
            panic!("get failed");
        };
        if let Ok((rx, initial)) = rx.await {
            Subscription { rx, initial }
        } else {
            error!("{}: get/await failed", self.name);
            panic!("get failed");
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
        U: TryFrom<T> + Clone + Eq + Send + 'static,
        <U as TryFrom<T>>::Error: Send + std::error::Error,
    {
        let name = format!("{} (translated)", self.name);
        let (tx, rx) = create_stateless_entity(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

            while let Ok(data) = sub.recv().await {
                match U::try_from(data) {
                    Ok(data) => {
                        tx.try_send(data);
                    }
                    Err(err) => {
                        error!("{}: parse failed: {}", name, err);
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
        let name = format!("{} (translated)", self.name);
        let (tx, rx) = create_stateful_entity(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

            while let Ok(data) = sub.recv().await {
                match U::try_from(data) {
                    Ok(data) => {
                        tx.try_send(data);
                    }
                    Err(err) => {
                        error!("{}: parse failed: {}", name, err);
                    }
                }
            }
        });

        rx
    }
}

impl<T: Send + Clone> Receiver<StatefulData<T>> {
    /// Get the most recent value from the entity.
    pub async fn get_data(&self) -> Option<T> {
        self.get().await.map(|(_prev, curr)| curr)
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
    initial: Option<T>,
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
    pub async fn recv_data(&mut self) -> Result<T, RecvError> {
        self.recv().await.map(|(_prev, curr)| curr)
    }

    /// Get the next value but don't wait for it. Returns `None` if there is no value.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub fn try_recv_data(&mut self) -> Result<Option<T>, RecvError> {
        self.try_recv().map(|opt| opt.map(|(_prev, curr)| curr))
    }
}

/// Create a stateless entity that sends every message.
#[must_use]
pub fn create_stateless_entity<T: Clone + Send + 'static>(name: &str) -> (Sender<T>, Receiver<T>) {
    let (send_tx, mut send_rx) = mpsc::channel::<SendMessage<T>>(PIPE_SIZE);
    let (receive_tx, mut receive_rx) = mpsc::channel::<ReceiveMessage<T>>(PIPE_SIZE);
    let (out_tx, out_rx) = broadcast::channel::<T>(PIPE_SIZE);

    drop(out_rx);

    let name = name.to_string();

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

        loop {
            select! {
                Some(msg) = send_rx.recv() => {
                    match msg {
                        SendMessage::Set(data) => {
                            saved_data = Some(data.clone());
                            if let Err(err) = out_tx.send(data) {
                                // It is not an error if there are no subscribers.
                                debug!("{name}: send to broadcast failed: {err} (not an error)");
                            }
                        }
                    }
                }
                Some(msg) = receive_rx.recv() => {
                    match msg {
                        ReceiveMessage::Get(tx) => {
                            if tx.send(saved_data.clone()).is_err() {
                                error!("{name}: get send failed");
                            };
                        }
                        ReceiveMessage::Subscribe(tx) => {
                            let rx = out_tx.subscribe();
                            if tx.send((rx, saved_data.clone())).is_err() {
                                error!("{name}: subscribe send failed");
                            };
                        }
                    }
                }
            }
        }
    });

    (sender, receiver)
}

/// Create a stateful entity that only produces messages when there is a change.
#[must_use]
pub fn create_stateful_entity<T: Clone + Eq + Send + 'static>(
    name: &str,
) -> (Sender<T>, Receiver<StatefulData<T>>) {
    let (send_tx, mut send_rx) = mpsc::channel::<SendMessage<T>>(PIPE_SIZE);
    let (receive_tx, mut receive_rx) = mpsc::channel::<ReceiveMessage<StatefulData<T>>>(PIPE_SIZE);
    let (out_tx, out_rx) = broadcast::channel::<StatefulData<T>>(PIPE_SIZE);

    drop(out_rx);

    let name = name.to_string();

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
        loop {
            select! {
                Some(msg) = send_rx.recv() => {
                    match msg {
                        SendMessage::Set(data) => {
                            let changed = match saved_data {
                                Some(ref saved_data) => data != *saved_data,
                                None => true,
                            };
                            if changed {
                                prev_data = saved_data.clone();
                                saved_data = Some(data.clone());
                                if let Err(err) = out_tx.send((prev_data.clone(), data)) {
                                    // It is not an error if there are no subscribers.
                                    debug!("{name}: send to broadcast failed: {err} (not an error)");
                                }
                            };
                        }
                    }
                }
                Some(msg) = receive_rx.recv() => {
                    match msg {
                        ReceiveMessage::Get(tx) => {
                            let data  = saved_data.clone().map(|data| (prev_data.clone(), data));
                            if tx.send(data).is_err() {
                                error!("{name}: get send failed");
                            };
                        }
                        ReceiveMessage::Subscribe(tx) => {
                            let data  = saved_data.clone().map(|data| (prev_data.clone(), data));
                            let rx = out_tx.subscribe();
                            if tx.send((rx, data)).is_err() {
                                error!("{name}: subscribe send failed");
                            };
                        }
                    }
                }
            }
        }
    });

    (sender, receiver)
}
#[cfg(test)]
mod tests {
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
    }
}
