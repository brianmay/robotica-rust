//! Stateless pipes do not track the current state of the entity.
pub mod receiver;
pub mod sender;

use std::sync::Arc;
use std::sync::OnceLock;

use arc_swap::ArcSwapAny;
pub use receiver::Receiver;
pub use receiver::Subscription;
pub use receiver::WeakReceiver;
use robotica_common::Duration;
pub use sender::Sender;
use tap::Pipe;
use tokio::select;
use tokio::sync::watch;
use tokio::time::sleep;
use tracing::debug;
use tracing::error;
use tracing::info;

use crate::spawn;

use super::PIPE_SIZE;
pub(super) use receiver::ReceiveMessage;
use sender::SendMessage;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

type StartedWatch = Arc<(watch::Sender<()>, watch::Receiver<()>)>;
static STARTED: OnceLock<ArcSwapAny<Option<StartedWatch>>> = OnceLock::new();

/// A stateless pipe that starts the stateless pipes after the application starts
pub struct Started();

impl Started {
    /// Create a new Started entity.
    pub fn new() -> Self {
        STARTED.get_or_init(|| {
            watch::channel(())
                .pipe(Arc::new)
                .pipe(Some)
                .pipe(ArcSwapAny::new)
        });
        Self()
    }

    /// Set the start of the stateless pipes.
    #[allow(clippy::unused_self)]
    pub fn notify(&self) {
        spawn(async {
            info!("stateless::notify: started notify");

            // After start give 5 seconds for subscribers to subscribe.
            sleep(Duration::from_secs(5)).await;

            let Some(arc_swap) = STARTED.get() else {
                error!("stateless::notify: not initialized");
                return;
            };

            let maybe_arc = arc_swap.load();
            let Some(arc) = maybe_arc.as_ref() else {
                error!("stateless::notify: dropped");
                return;
            };

            let (tx, _rx) = arc.as_ref();
            if tx.send(()).is_err() {
                error!("stateless::notify: error sending");
            }

            info!("stateless::notify: started done");
        });
    }

    #[must_use]
    fn subscribe() -> Option<watch::Receiver<()>> {
        let Some(arc_swap) = STARTED.get() else {
            info!("stateless::subscribe: not initialized");
            return None;
        };

        let maybe_arc = arc_swap.load();
        let Some(arc) = maybe_arc.as_ref() else {
            error!("stateless::subscribe: dropped");
            return None;
        };

        let (_tx, rx) = arc.as_ref();
        Some(rx.clone())
    }
}

impl Default for Started {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Started {
    fn drop(&mut self) {
        match STARTED.get() {
            Some(arc_swap) => {
                arc_swap.store(None);
            }
            None => {
                error!("stateless::set_start: not initialized");
            }
        }
    }
}

/// Create a stateless entity that sends every message.
#[must_use]
pub fn create_pipe<T>(name: impl Into<String>) -> (Sender<T>, Receiver<T>)
where
    T: Clone + Send + 'static,
{
    let (send_tx, send_rx) = mpsc::channel::<SendMessage<T>>(PIPE_SIZE);
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
        let mut send_rx = send_rx;
        let mut receive_rx = receive_rx;
        let mut started = Started::subscribe();
        let mut has_started = started.is_none();

        loop {
            select! {
                // This is a hack to allow the sender to send a message before the receiver has
                // subscribed.
                Some(msg) = recv_if_started(&mut send_rx, has_started) => {
                    #[allow(clippy::single_match_else)]
                    match msg {
                        Some(SendMessage::Set(data)) => {
                            if let Err(_err) = out_tx.send(data) {
                                // It is not an error if there are no subscribers.
                            }
                        }
                        None => {
                            debug!("stateless::create_pipe({name}): send channel closed");
                            break;
                        }
                    }
                }
                msg = receive_rx.recv() => {
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
                            debug!("stateless::create_pipe({name}): receive channel closed");
                            break;
                        }
                    }
                }
                Some(Ok(())) = await_started(&mut started) => {
                    started = None;
                    has_started = true;
                }
                else => {
                    debug!("stateless::create_pipe({name}): all inputs closed");
                    break;
                }
            }
        }
    });

    (sender, receiver)
}

async fn recv_if_started<T: Send>(
    rx: &mut mpsc::Receiver<T>,
    has_started: bool,
) -> Option<Option<T>> {
    if has_started {
        rx.recv().await.pipe(Some)
    } else {
        None
    }
}

async fn await_started(
    rx: &mut Option<watch::Receiver<()>>,
) -> Option<Result<(), watch::error::RecvError>> {
    if let Some(rx) = rx {
        rx.changed().await.pipe(Some)
    } else {
        None
    }
}
