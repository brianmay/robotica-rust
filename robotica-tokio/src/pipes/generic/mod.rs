//! An indexed pipe stores the last value per key (via `HasIndex`) and
//! can be turned into a stateful or stateless receiver.
pub mod receiver;
pub mod sender;

use std::collections::HashMap;

pub use receiver::Receiver;
pub use receiver::WeakReceiver;
pub use sender::Sender;
use tokio::select;
use tracing::debug;
use tracing::error;

use robotica_common::mqtt::HasIndex;

use crate::pipes::stateful::receiver::OldNewType;
use crate::spawn;

use self::receiver::ReceiveMessage;

use super::PIPE_SIZE;
use sender::SendMessage;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

/// Create an indexed entity that stores the last value per key and replays
/// all current values to new subscribers.
///
/// Keys are extracted via [`HasIndex::has_index`].  If `has_index()` returns
/// `None`, the value is stored under a singleton key.
#[must_use]
pub fn create_pipe<T>(name: impl Into<String>) -> (Sender<T>, Receiver<T>)
where
    T: Clone + PartialEq + Send + HasIndex + 'static,
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
        let mut indexed_data: HashMap<String, T> = HashMap::new();
        let mut send_rx = send_rx;
        let mut receive_rx = receive_rx;

        loop {
            select! {
                msg = send_rx.recv() => {
                    #[allow(clippy::single_match_else)]
                    match msg {
                        Some(SendMessage::Set(data)) => {
                            let key = data.has_index().unwrap_or_else(|| "_singleton".to_string());
                            let prev_data = indexed_data.get(&key).cloned();
                            let changed = prev_data.as_ref().is_none_or(|saved| saved != &data);
                            if changed {
                                indexed_data.insert(key, data.clone());
                            }
                            // Always broadcast — stateless subscribers need
                            // every message, even when the per-key value is unchanged.
                            if let Err(_err) = out_tx.send((prev_data, data)) {
                                // It is not an error if there are no subscribers.
                            }
                        }
                        None => {
                            debug!("generic::create_pipe({name}): send channel closed");
                            break;
                        }
                    }
                }
                msg = receive_rx.recv() => {
                    #[allow(clippy::single_match_else)]
                    match msg {
                        Some(ReceiveMessage::Get(tx)) => {
                            let data = indexed_data.values().last().cloned();
                            if tx.send(data).is_err() {
                                error!("generic::create_pipe({name}): get send failed");
                            }
                        }
                        Some(ReceiveMessage::Subscribe(tx)) => {
                            let rx = out_tx.subscribe();
                            let replay: Vec<T> = indexed_data.values().cloned().collect();
                            if tx.send((rx, replay)).is_err() {
                                error!("generic::create_pipe({name}): subscribe send failed");
                            }
                        }
                        None => {
                            debug!("generic::create_pipe({name}): receive channel closed");
                            break;
                        }
                    }
                }
                else => {
                    debug!("generic::create_pipe({name}): all inputs closed");
                    break;
                }
            }
        }
    });

    (sender, receiver)
}
