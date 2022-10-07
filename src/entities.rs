//! Stream of data from a source.

use log::debug;
use log::error;
use thiserror::Error;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::spawn;
use crate::PIPE_SIZE;

/// Incoming previous value and current value.
pub type Data<T> = (Option<T>, T);

enum Message<T> {
    Set(T),
    Get(oneshot::Sender<Option<T>>),
    Subscribe(oneshot::Sender<(broadcast::Receiver<Data<T>>, Option<T>)>),
}

/// Send a value to an entity.
#[derive(Clone)]
pub struct Sender<T> {
    #[allow(dead_code)]
    name: String,
    tx: mpsc::Sender<Message<T>>,
}

impl<T: Send> Sender<T> {
    /// Send data to the entity.
    pub async fn send(&self, data: T) {
        let msg = Message::Set(data);
        if let Err(err) = self.tx.send(msg).await {
            error!("send failed: {}", err);
        }
    }

    /// Send data to the entity or fail if buffer is full.
    pub fn try_send(&self, data: T) {
        let msg = Message::Set(data);
        if let Err(err) = self.tx.try_send(msg) {
            error!("send failed: {}", err);
        }
    }
}

/// Receive a value from an entity.
#[derive(Clone)]
pub struct Receiver<T> {
    name: String,
    tx: mpsc::Sender<Message<T>>,
}

impl<T: Send + Clone> Receiver<T> {
    /// Retrieve the most recent value from the entity.
    ///
    /// # Panics
    ///
    /// Panics if the receiver is disconnected.
    pub async fn get(&self) -> Option<T> {
        let (tx, rx) = oneshot::channel();
        let msg = Message::Get(tx);
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
        let msg = Message::Subscribe(tx);
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

    /// Translate this receiver into a another type.
    #[must_use]
    pub fn translate_into<U>(self) -> Receiver<U>
    where
        T: Send + 'static,
        U: TryFrom<T> + Clone + Eq + Send + 'static,
        <U as TryFrom<T>>::Error: Send + std::error::Error,
    {
        let name = format!("{} (translated)", self.name);
        let (tx, rx) = create_entity(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

            while let Ok((_, data)) = sub.recv().await {
                match U::try_from(data) {
                    Ok(data) => {
                        tx.send(data).await;
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

/// Something went wrong in Receiver.
#[derive(Error, Debug)]
pub enum RecvError {
    /// The Pipe was closed.
    #[error("The pipe was closed")]
    Closed,
}

/// A subscription to receive data from an entity.
pub struct Subscription<T> {
    rx: broadcast::Receiver<Data<T>>,
    initial: Option<T>,
}

impl<T: Send + Clone> Subscription<T> {
    /// Wait for the next value from the entity.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub async fn recv(&mut self) -> Result<Data<T>, RecvError> {
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
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub fn try_recv(&mut self) -> Result<Option<Data<T>>, RecvError> {
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

/// Create an entity.
#[must_use]
pub fn create_entity<T: Clone + Eq + Send + 'static>(name: &str) -> (Sender<T>, Receiver<T>) {
    let (in_tx, mut in_rx) = mpsc::channel::<Message<T>>(PIPE_SIZE);
    let (out_tx, out_rx) = broadcast::channel::<Data<T>>(PIPE_SIZE);

    drop(out_rx);

    let name = name.to_string();

    let sender = Sender {
        tx: in_tx.clone(),
        name: name.clone(),
    };
    let receiver = Receiver {
        tx: in_tx,
        name: name.clone(),
    };

    spawn(async move {
        let name = name;

        let mut saved_data: Option<T> = None;
        while let Some(msg) = in_rx.recv().await {
            match msg {
                Message::Set(data) => {
                    let changed = match saved_data {
                        Some(ref saved_data) => data != *saved_data,
                        None => true,
                    };
                    if changed {
                        let prev_data = saved_data.clone();
                        saved_data = Some(data.clone());
                        if let Err(err) = out_tx.send((prev_data, data)) {
                            // It is not an error if there are no subscribers.
                            debug!("{name}: send to broadcast failed: {err} (not an error)");
                        }
                    };
                }
                Message::Get(tx) => {
                    if tx.send(saved_data.clone()).is_err() {
                        error!("{name}: get send failed");
                    };
                }
                Message::Subscribe(tx) => {
                    let rx = out_tx.subscribe();
                    if tx.send((rx, saved_data.clone())).is_err() {
                        error!("{name}: subscribe send failed");
                    };
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
    async fn test_entity() {
        let (tx, rx) = create_entity::<String>("test");
        tx.send("hello".to_string()).await;
        let mut s = rx.subscribe().await;
        tx.send("goodbye".to_string()).await;

        let (prev, current) = s.recv().await.unwrap();
        assert_eq!(None, prev);
        assert_eq!("hello", current);

        let (prev, current) = s.recv().await.unwrap();
        assert_eq!("hello", prev.unwrap());
        assert_eq!("goodbye", current);

        let result = rx.get().await;
        assert_eq!(Some("goodbye".to_string()), result);

        let result = s.try_recv().unwrap();
        assert!(result.is_none());

        let result = rx.get().await;
        assert_eq!(Some("goodbye".to_string()), result);
    }
}
