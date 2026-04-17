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

use crate::pipes::{RecvError, Subscriber};
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
    let (send_tx, send_rx) = mpsc::unbounded_channel::<SendMessage<T>>();
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
    let (send_tx, send_rx) = mpsc::unbounded_channel::<SendMessage<T>>();
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

/// Combine multiple stateful receivers into one that emits the changed input's index and value.
///
/// When any input changes, outputs `(input_index, new_value)`.
#[must_use]
pub fn combine_latest<T>(name: &str, receivers: Vec<Receiver<T>>) -> Receiver<(usize, T)>
where
    T: Clone + PartialEq + Send + 'static,
{
    let (tx_out, rx_out) = create_pipe(name);
    let name = name.to_string();

    spawn(async move {
        let (chan_tx, mut chan_rx) = tokio::sync::mpsc::channel::<(usize, T)>(receivers.len());

        for (i, rx) in receivers.into_iter().enumerate() {
            let chan_tx = chan_tx.clone();
            let name = name.clone();
            spawn(async move {
                let mut sub = rx.subscribe().await;
                loop {
                    match sub.recv_old_new().await {
                        Ok((_, new_value)) => {
                            if chan_tx.send((i, new_value)).await.is_err() {
                                debug!("{name}: combined channel closed, exiting");
                                break;
                            }
                        }
                        Err(RecvError::Closed) => {
                            debug!("{name}: source closed, exiting");
                            break;
                        }
                    }
                }
            });
        }
        drop(chan_tx);

        while let Some((i, value)) = chan_rx.recv().await {
            tx_out.try_send((i, value));
        }
    });

    rx_out
}

/// Combine multiple stateful receivers into one that emits the changed input's index and value, with a custom combine function.
#[must_use]
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::similar_names)]
pub fn combine_latest_2<T1, T2>(
    name: &str,
    rx1: Receiver<T1>,
    rx2: Receiver<T2>,
) -> Receiver<(Option<T1>, Option<T2>)>
where
    T1: Clone + PartialEq + Send + 'static,
    T2: Clone + PartialEq + Send + 'static,
{
    let (tx_out, rx_out) = create_pipe(name);
    let name = name.to_string();

    spawn(async move {
        let (in_tx1, mut in_rx1) = tokio::sync::mpsc::channel::<T1>(1);
        let (in_tx2, mut in_rx2) = tokio::sync::mpsc::channel::<T2>(1);

        let mut sub1 = rx1.subscribe().await;
        let mut sub2 = rx2.subscribe().await;

        let name1 = name.clone();
        let name2 = name.clone();
        let in_tx1 = in_tx1.clone();
        let in_tx2 = in_tx2.clone();

        spawn(async move {
            loop {
                match sub1.recv_old_new().await {
                    Ok((_, value)) => {
                        if in_tx1.send(value).await.is_err() {
                            debug!("{name1}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name1}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        spawn(async move {
            loop {
                match sub2.recv_old_new().await {
                    Ok((_, value)) => {
                        if in_tx2.send(value).await.is_err() {
                            debug!("{name2}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name2}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        let mut latest1: Option<T1> = None;
        let mut latest2: Option<T2> = None;

        loop {
            tokio::select! {
                Some(v1) = in_rx1.recv() => {
                    latest1 = Some(v1);
                    tx_out.try_send((latest1.clone(), latest2.clone()));
                }
                Some(v2) = in_rx2.recv() => {
                    latest2 = Some(v2);
                    tx_out.try_send((latest1.clone(), latest2.clone()));
                }
                else => break,
            }
        }
    });

    rx_out
}

#[expect(dead_code)]
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::similar_names)]
fn combine_latest_3<T1, T2, T3>(
    name: &str,
    rx1: Receiver<T1>,
    rx2: Receiver<T2>,
    rx3: Receiver<T3>,
) -> Receiver<(Option<T1>, Option<T2>, Option<T3>)>
where
    T1: Clone + PartialEq + Send + 'static,
    T2: Clone + PartialEq + Send + 'static,
    T3: Clone + PartialEq + Send + 'static,
{
    let (tx_out, rx_out) = create_pipe(name);
    let name = name.to_string();

    spawn(async move {
        let (chan_tx1, mut chan_rx1) = tokio::sync::mpsc::channel::<T1>(1);
        let (chan_tx2, mut chan_rx2) = tokio::sync::mpsc::channel::<T2>(1);
        let (chan_tx3, mut chan_rx3) = tokio::sync::mpsc::channel::<T3>(1);

        let mut sub1 = rx1.subscribe().await;
        let mut sub2 = rx2.subscribe().await;
        let mut sub3 = rx3.subscribe().await;

        let name1 = name.clone();
        let name2 = name.clone();
        let name3 = name.clone();
        let chan_tx1 = chan_tx1.clone();
        let chan_tx2 = chan_tx2.clone();
        let chan_tx3 = chan_tx3.clone();

        spawn(async move {
            loop {
                match sub1.recv_old_new().await {
                    Ok((_, value)) => {
                        if chan_tx1.send(value).await.is_err() {
                            debug!("{name1}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name1}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        spawn(async move {
            loop {
                match sub2.recv_old_new().await {
                    Ok((_, value)) => {
                        if chan_tx2.send(value).await.is_err() {
                            debug!("{name2}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name2}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        spawn(async move {
            loop {
                match sub3.recv_old_new().await {
                    Ok((_, value)) => {
                        if chan_tx3.send(value).await.is_err() {
                            debug!("{name3}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name3}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        let mut latest1: Option<T1> = None;
        let mut latest2: Option<T2> = None;
        let mut latest3: Option<T3> = None;

        loop {
            tokio::select! {
                Some(v1) = chan_rx1.recv() => {
                    latest1 = Some(v1);
                    tx_out.try_send((latest1.clone(), latest2.clone(), latest3.clone()));
                }
                Some(v2) = chan_rx2.recv() => {
                    latest2 = Some(v2);
                    tx_out.try_send((latest1.clone(), latest2.clone(), latest3.clone()));
                }
                Some(v3) = chan_rx3.recv() => {
                    latest3 = Some(v3);
                    tx_out.try_send((latest1.clone(), latest2.clone(), latest3.clone()));
                }
                else => break,
            }
        }
    });

    rx_out
}

/// Combine multiple stateful receivers into one that emits the changed input's index and value, with a custom combine function.
#[must_use]
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::similar_names)]
pub fn combine_latest_4<T1, T2, T3, T4>(
    name: &str,
    rx1: Receiver<T1>,
    rx2: Receiver<T2>,
    rx3: Receiver<T3>,
    rx4: Receiver<T4>,
) -> Receiver<(Option<T1>, Option<T2>, Option<T3>, Option<T4>)>
where
    T1: Clone + PartialEq + Send + 'static,
    T2: Clone + PartialEq + Send + 'static,
    T3: Clone + PartialEq + Send + 'static,
    T4: Clone + PartialEq + Send + 'static,
{
    let (tx_out, rx_out) = create_pipe(name);
    let name = name.to_string();

    spawn(async move {
        let (chan_tx1, mut chan_rx1) = tokio::sync::mpsc::channel::<T1>(1);
        let (chan_tx2, mut chan_rx2) = tokio::sync::mpsc::channel::<T2>(1);
        let (chan_tx3, mut chan_rx3) = tokio::sync::mpsc::channel::<T3>(1);
        let (chan_tx4, mut chan_rx4) = tokio::sync::mpsc::channel::<T4>(1);

        let mut sub1 = rx1.subscribe().await;
        let mut sub2 = rx2.subscribe().await;
        let mut sub3 = rx3.subscribe().await;
        let mut sub4 = rx4.subscribe().await;

        let name1 = name.clone();
        let name2 = name.clone();
        let name3 = name.clone();
        let name4 = name.clone();
        let chan_tx1 = chan_tx1.clone();
        let chan_tx2 = chan_tx2.clone();
        let chan_tx3 = chan_tx3.clone();
        let chan_tx4 = chan_tx4.clone();

        spawn(async move {
            loop {
                match sub1.recv_old_new().await {
                    Ok((_, value)) => {
                        if chan_tx1.send(value).await.is_err() {
                            debug!("{name1}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name1}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        spawn(async move {
            loop {
                match sub2.recv_old_new().await {
                    Ok((_, value)) => {
                        if chan_tx2.send(value).await.is_err() {
                            debug!("{name2}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name2}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        spawn(async move {
            loop {
                match sub3.recv_old_new().await {
                    Ok((_, value)) => {
                        if chan_tx3.send(value).await.is_err() {
                            debug!("{name3}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name3}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        spawn(async move {
            loop {
                match sub4.recv_old_new().await {
                    Ok((_, value)) => {
                        if chan_tx4.send(value).await.is_err() {
                            debug!("{name4}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name4}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        let mut latest1: Option<T1> = None;
        let mut latest2: Option<T2> = None;
        let mut latest3: Option<T3> = None;
        let mut latest4: Option<T4> = None;

        loop {
            tokio::select! {
                Some(v1) = chan_rx1.recv() => {
                    latest1 = Some(v1);
                    tx_out.try_send((latest1.clone(), latest2.clone(), latest3.clone(), latest4.clone()));
                }
                Some(v2) = chan_rx2.recv() => {
                    latest2 = Some(v2);
                    tx_out.try_send((latest1.clone(), latest2.clone(), latest3.clone(), latest4.clone()));
                }
                Some(v3) = chan_rx3.recv() => {
                    latest3 = Some(v3);
                    tx_out.try_send((latest1.clone(), latest2.clone(), latest3.clone(), latest4.clone()));
                }
                Some(v4) = chan_rx4.recv() => {
                    latest4 = Some(v4);
                    tx_out.try_send((latest1.clone(), latest2.clone(), latest3.clone(), latest4.clone()));
                }
                else => break,
            }
        }
    });

    rx_out
}

/// Combine multiple stateful receivers into one that emits the changed input's index and value, with a custom combine function.
#[must_use]
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::similar_names)]
pub fn combine_latest_5<T1, T2, T3, T4, T5>(
    name: &str,
    rx1: Receiver<T1>,
    rx2: Receiver<T2>,
    rx3: Receiver<T3>,
    rx4: Receiver<T4>,
    rx5: Receiver<T5>,
) -> Receiver<(Option<T1>, Option<T2>, Option<T3>, Option<T4>, Option<T5>)>
where
    T1: Clone + PartialEq + Send + 'static,
    T2: Clone + PartialEq + Send + 'static,
    T3: Clone + PartialEq + Send + 'static,
    T4: Clone + PartialEq + Send + 'static,
    T5: Clone + PartialEq + Send + 'static,
{
    let (tx_out, rx_out) = create_pipe(name);
    let name = name.to_string();

    spawn(async move {
        let (chan_tx1, mut chan_rx1) = tokio::sync::mpsc::channel::<T1>(1);
        let (chan_tx2, mut chan_rx2) = tokio::sync::mpsc::channel::<T2>(1);
        let (chan_tx3, mut chan_rx3) = tokio::sync::mpsc::channel::<T3>(1);
        let (chan_tx4, mut chan_rx4) = tokio::sync::mpsc::channel::<T4>(1);
        let (chan_tx5, mut chan_rx5) = tokio::sync::mpsc::channel::<T5>(1);

        let mut sub1 = rx1.subscribe().await;
        let mut sub2 = rx2.subscribe().await;
        let mut sub3 = rx3.subscribe().await;
        let mut sub4 = rx4.subscribe().await;
        let mut sub5 = rx5.subscribe().await;

        let name1 = name.clone();
        let name2 = name.clone();
        let name3 = name.clone();
        let name4 = name.clone();
        let name5 = name.clone();
        let chan_tx1 = chan_tx1.clone();
        let chan_tx2 = chan_tx2.clone();
        let chan_tx3 = chan_tx3.clone();
        let chan_tx4 = chan_tx4.clone();
        let chan_tx5 = chan_tx5.clone();

        spawn(async move {
            loop {
                match sub1.recv_old_new().await {
                    Ok((_, value)) => {
                        if chan_tx1.send(value).await.is_err() {
                            debug!("{name1}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name1}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        spawn(async move {
            loop {
                match sub2.recv_old_new().await {
                    Ok((_, value)) => {
                        if chan_tx2.send(value).await.is_err() {
                            debug!("{name2}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name2}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        spawn(async move {
            loop {
                match sub3.recv_old_new().await {
                    Ok((_, value)) => {
                        if chan_tx3.send(value).await.is_err() {
                            debug!("{name3}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name3}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        spawn(async move {
            loop {
                match sub4.recv_old_new().await {
                    Ok((_, value)) => {
                        if chan_tx4.send(value).await.is_err() {
                            debug!("{name4}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name4}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        spawn(async move {
            loop {
                match sub5.recv_old_new().await {
                    Ok((_, value)) => {
                        if chan_tx5.send(value).await.is_err() {
                            debug!("{name5}: channel closed, exiting");
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        debug!("{name5}: source closed, exiting");
                        break;
                    }
                }
            }
        });

        let mut latest1: Option<T1> = None;
        let mut latest2: Option<T2> = None;
        let mut latest3: Option<T3> = None;
        let mut latest4: Option<T4> = None;
        let mut latest5: Option<T5> = None;

        loop {
            tokio::select! {
                Some(v1) = chan_rx1.recv() => {
                    latest1 = Some(v1);
                    tx_out.try_send((latest1.clone(), latest2.clone(), latest3.clone(), latest4.clone(), latest5.clone()));
                }
                Some(v2) = chan_rx2.recv() => {
                    latest2 = Some(v2);
                    tx_out.try_send((latest1.clone(), latest2.clone(), latest3.clone(), latest4.clone(), latest5.clone()));
                }
                Some(v3) = chan_rx3.recv() => {
                    latest3 = Some(v3);
                    tx_out.try_send((latest1.clone(), latest2.clone(), latest3.clone(), latest4.clone(), latest5.clone()));
                }
                Some(v4) = chan_rx4.recv() => {
                    latest4 = Some(v4);
                    tx_out.try_send((latest1.clone(), latest2.clone(), latest3.clone(), latest4.clone(), latest5.clone()));
                }
                Some(v5) = chan_rx5.recv() => {
                    latest5 = Some(v5);
                    tx_out.try_send((latest1.clone(), latest2.clone(), latest3.clone(), latest4.clone(), latest5.clone()));
                }
                else => break,
            }
        }
    });

    rx_out
}

impl<State> Receiver<State>
where
    State: Clone + PartialEq + Send + 'static,
{
    /// Combine two stateful receivers into one that emits the changed input's index and value.
    ///
    /// When either input changes, outputs `(input_index, new_value)`.
    #[must_use]
    pub fn combine_latest(self, other: Receiver<State>) -> Receiver<(usize, State)> {
        crate::pipes::stateful::combine_latest(
            &format!("{} (combine_latest)", self.name),
            vec![self, other],
        )
    }

    /// Combine multiple stateful receivers into one that emits the changed input's index and value.
    ///
    /// When any input changes, outputs `(input_index, new_value)`.
    #[must_use]
    pub fn combine_latest_many(self, others: Vec<Receiver<State>>) -> Receiver<(usize, State)> {
        let mut all_receivers = vec![self];
        all_receivers.extend(others);
        crate::pipes::stateful::combine_latest("combine_latest_many", all_receivers)
    }
}
