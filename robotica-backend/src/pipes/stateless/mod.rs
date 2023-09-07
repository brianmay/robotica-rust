//! Stateless pipes do not track the current state of the entity.
pub mod receiver;
pub mod sender;

pub use receiver::Receiver;
pub use receiver::Subscription;
pub use receiver::WeakReceiver;
pub use sender::Sender;
use tokio::select;
use tracing::debug;
use tracing::error;

use crate::pipes::try_receive;
use crate::spawn;

use super::PIPE_SIZE;
pub(super) use receiver::ReceiveMessage;
use sender::SendMessage;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

/// Create a stateless entity that sends every message.
#[must_use]
pub fn create_pipe<T>(name: impl Into<String>) -> (Sender<T>, Receiver<T>)
where
    T: Clone + Send + 'static,
{
    let (send_tx, mut send_rx) = mpsc::channel::<SendMessage<T>>(PIPE_SIZE);
    let (receive_tx, receive_rx) = mpsc::channel::<ReceiveMessage<T>>(PIPE_SIZE);
    let (out_tx, out_rx) = broadcast::channel::<T>(PIPE_SIZE);

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
        let name = name;
        let mut receive_rx = Some(receive_rx);

        loop {
            select! {
                Some(msg) = send_rx.recv() => {
                    match msg {
                        SendMessage::Set(data) => {
                            if let Err(_err) = out_tx.send(data) {
                                // It is not an error if there are no subscribers.
                                // debug!("stateless::create_pipe({name}): send to broadcast failed: {err} (not an error)");
                            }
                        }
                    }
                }
                Some(msg) = try_receive(&mut receive_rx) => {
                    // This should create an error if we add messages in the future.
                    #[allow(clippy::single_match_else)]
                    match msg {
                        Some(ReceiveMessage::Subscribe(tx)) => {
                            let rx = out_tx.subscribe();
                            if tx.send(rx).is_err() {
                                error!("stateless::create_pipe({name}): subscribe send failed");
                            };
                        }
                        None => {
                            debug!("stateless::create_pipe({name}): command channel closed");
                            receive_rx = None;
                        }
                    }
                }
                else => {
                    debug!("stateless::create_pipe({name}): all inputs closed");
                    break;
                }
            }

            if matches!((&receive_rx, out_tx.receiver_count()), (None, 0)) {
                debug!(
                    "stateless::create_pipe({name}): receiver closed and all subscriptions closed"
                );
                break;
            }
        }
    });

    (sender, receiver)
}
