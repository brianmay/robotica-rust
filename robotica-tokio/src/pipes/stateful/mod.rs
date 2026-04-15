//! Stateful pipes track the current state of the entity.
pub mod receiver;
pub mod sender;

pub use receiver::OldNewType;
pub use receiver::Receiver;
pub use receiver::Subscription;
pub use receiver::WeakReceiver;
pub use sender::Sender;
use std::collections::HashMap;
use tokio::select;
use tracing::debug;
use tracing::error;

use robotica_common::mqtt::HasIndex;

use crate::spawn;

use super::{MAX_INDEXED_REPLAY_SIZE, PIPE_SIZE};
pub(in crate::pipes) use receiver::ReceiveMessage;
use sender::SendMessage;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

/// Create a stateful entity that sends every message and adds state messages.
/// Only the last value is stored and replayed to new subscribers.
#[must_use]
pub fn create_pipe<T>(name: impl Into<String>) -> (Sender<T>, Receiver<T>)
where
    T: Clone + PartialEq + Send + 'static,
{
    let (send_tx, send_rx) = mpsc::channel::<SendMessage<T>>(PIPE_SIZE);
    let (receive_tx, receive_rx) = mpsc::channel::<ReceiveMessage<T>>(PIPE_SIZE);
    let (out_tx, out_rx) = broadcast::channel::<OldNewType<T>>(PIPE_SIZE);

    drop(out_rx);

    let name = name.into();

    let sender: Sender<T> = Sender {
        tx: send_tx,
        name: name.clone(),
    };
    let receiver: Receiver<T> = Receiver {
        tx: receive_tx,
        name: name.clone(),
    };

    spawn(async move {
        let mut current_data: Option<T> = None;
        let mut send_rx = send_rx;
        let mut receive_rx = receive_rx;

        loop {
            select! {
                msg = send_rx.recv() => {
                    match msg {
                        Some(SendMessage::Set(data)) => {
                            let changed = current_data.as_ref().is_none_or(|saved_data| data != *saved_data);
                            if changed {
                                let prev_data = current_data.clone();
                                current_data = Some(data.clone());
                                if let Err(_err) = out_tx.send((prev_data, data)) {
                                    // It is not an error if there are no subscribers.
                                }
                            }
                        }
                        None => {
                            debug!("stateful::create_pipe({name}): send channel closed");
                            break;
                        }
                    }
                }
                msg = receive_rx.recv() => {
                    match msg {
                        Some(ReceiveMessage::Get(tx)) => {
                            let data = current_data.clone();
                            if tx.send(data).is_err() {
                                error!("stateful::create_pipe({name}): get send failed");
                            }
                        }
                        Some(ReceiveMessage::Subscribe(tx)) => {
                            let rx = out_tx.subscribe();
                            let replay = current_data.clone().into_iter().collect();
                            if tx.send((rx, replay)).is_err() {
                                error!("stateful::create_pipe{name}): subscribe send failed");
                            }
                        }
                        None => {
                            debug!("stateful::create_pipe({name}): receive channel closed");
                            break;
                        }
                    }
                }
                else => {
                    debug!("stateful::create_pipe({name}): all inputs closed");
                    break;
                }
            }
        }
    });

    (sender, receiver)
}

/// Create a stateful indexed entity that sends every message and adds state messages.
///
/// Values are stored in a `HashMap` keyed by the index returned by `has_index()`.
/// If `has_index()` returns `Some(key)`, values are stored per-key with bounded replay.
/// If `has_index()` returns `None`, values are stored under a singleton key.
#[must_use]
pub fn create_indexed_pipe<T>(name: impl Into<String>) -> (Sender<T>, Receiver<T>)
where
    T: Clone + PartialEq + Send + 'static + HasIndex,
{
    let (send_tx, send_rx) = mpsc::channel::<SendMessage<T>>(PIPE_SIZE);
    let (receive_tx, receive_rx) = mpsc::channel::<ReceiveMessage<T>>(PIPE_SIZE);
    let (out_tx, out_rx) = broadcast::channel::<OldNewType<T>>(PIPE_SIZE);

    drop(out_rx);

    let name = name.into();

    let sender: Sender<T> = Sender {
        tx: send_tx,
        name: name.clone(),
    };
    let receiver: Receiver<T> = Receiver {
        tx: receive_tx,
        name: name.clone(),
    };

    spawn(async move {
        let mut indexed_data: HashMap<String, Vec<T>> = HashMap::new();
        let mut send_rx = send_rx;
        let mut receive_rx = receive_rx;

        loop {
            select! {
                msg = send_rx.recv() => {
                    match msg {
                        Some(SendMessage::Set(data)) => {
                            let key = data.has_index().unwrap_or_else(|| "_singleton".to_string());
                            let prev_data = indexed_data.get(&key).and_then(|v| v.last()).cloned();
                            indexed_data.entry(key.clone()).or_default();
                            if let Some(v) = indexed_data.get_mut(&key) {
                                v.push(data.clone());
                                if v.len() > MAX_INDEXED_REPLAY_SIZE {
                                    v.remove(0);
                                }
                            }
                            if let Err(_err) = out_tx.send((prev_data, data)) {
                                // It is not an error if there are no subscribers.
                            }
                        }
                        None => {
                            debug!("stateful::create_indexed_pipe({name}): send channel closed");
                            break;
                        }
                    }
                }
                msg = receive_rx.recv() => {
                    match msg {
                        Some(ReceiveMessage::Get(tx)) => {
                            let data = indexed_data.values().last().and_then(|v| v.last()).cloned();
                            if tx.send(data).is_err() {
                                error!("stateful::create_indexed_pipe({name}): get send failed");
                            }
                        }
                        Some(ReceiveMessage::Subscribe(tx)) => {
                            let rx = out_tx.subscribe();
                            let replay: Vec<T> = indexed_data
                                .values()
                                .filter_map(|v| v.last().cloned())
                                .collect();
                            if tx.send((rx, replay)).is_err() {
                                error!("stateful::create_indexed_pipe{name}): subscribe send failed");
                            }
                        }
                        None => {
                            debug!("stateful::create_indexed_pipe({name}): receive channel closed");
                            break;
                        }
                    }
                }
                else => {
                    debug!("stateful::create_indexed_pipe({name}): all inputs closed");
                    break;
                }
            }
        }
    });

    (sender, receiver)
}

/// Create a stateful entity that sends a static value.
pub fn static_pipe<T: Clone + PartialEq + Send + 'static>(
    value: T,
    name: impl Into<String>,
) -> Receiver<T> {
    let (tx, rx) = create_pipe(name);
    spawn(async move {
        tx.try_send(value);
        tx.closed().await;
    });
    rx
}

/// Create a pipe that always returns the same value.
pub fn static_entity<T: 'static + Clone + PartialEq + Send>(
    pc: T,
    name: impl Into<String>,
) -> Receiver<T> {
    let (tx, rx) = create_pipe(name);
    spawn(async move {
        tx.try_send(pc);
        tx.closed().await;
    });
    rx
}
