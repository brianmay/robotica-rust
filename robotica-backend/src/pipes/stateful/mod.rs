//! Stateless pipes do not track the current state of the entity.
pub mod receiver;
pub mod sender;

pub use receiver::OldNewType;
pub use receiver::Receiver;
pub use receiver::Subscription;
pub use receiver::WeakReceiver;
pub use sender::Sender;
use tokio::select;
use tracing::debug;
use tracing::error;

use crate::spawn;

use super::PIPE_SIZE;
pub(in crate::pipes) use receiver::ReceiveMessage;
use sender::SendMessage;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

/// Create a stateful entity that sends every message and adds state messages
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
                    #[allow(clippy::single_match_else)]
                    match msg {
                        Some(SendMessage::Set(data)) => {
                            let changed = current_data.as_ref().map_or(true, |saved_data| data != *saved_data);
                            if changed {
                                let prev_data = current_data;
                                current_data = Some(data.clone());
                                if let Err(_err) = out_tx.send((prev_data.clone(), data)) {
                                    // It is not an error if there are no subscribers.
                                }
                            };
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
                            };
                        }
                        Some(ReceiveMessage::Subscribe(tx)) => {
                            let rx = out_tx.subscribe();
                            if tx.send((rx, current_data.clone())).is_err() {
                                error!("stateful::create_pipe{name}): subscribe send failed");
                            };
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
