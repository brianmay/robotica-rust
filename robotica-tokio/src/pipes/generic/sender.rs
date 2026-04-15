//! Generic sender code.
use tokio::sync::mpsc;
use tracing::error;

pub(super) enum SendMessage<T> {
    Set(T),
}

/// Send a value to an entity.
#[derive(Clone)]
pub struct Sender<T> {
    #[allow(dead_code)]
    pub(super) name: String,
    pub(super) tx: mpsc::UnboundedSender<SendMessage<T>>,
}

impl<T> Sender<T> {
    /// Send data to the entity.
    pub fn try_send(&self, data: T) {
        let msg = SendMessage::Set(data);
        if let Err(err) = self.tx.send(msg) {
            error!("send failed({}): {}", self.name, err);
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
