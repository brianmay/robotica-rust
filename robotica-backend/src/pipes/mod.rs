//! Enhanced message queues

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;

pub mod generic;
pub mod stateful;
pub mod stateless;

/// Size of all pipes.
pub const PIPE_SIZE: usize = 10;

async fn try_receive<T: Send>(rx: &mut Option<mpsc::Receiver<T>>) -> Option<Option<T>> {
    match rx {
        Some(rx) => Some(rx.recv().await),
        None => None,
    }
}

/// Something went wrong in Receiver.
#[derive(Error, Debug)]
pub enum RecvError {
    /// The Pipe was closed.
    #[error("The pipe was closed")]
    Closed,
}

/// Allow subscribing to an pipe
#[async_trait]
pub trait Subscriber<T> {
    /// The type of the subscription
    type SubscriptionType: Subscription<T> + Send + 'static;

    /// Subscribe to a pipe
    ///
    /// Returns an already closed subscription if the entity is closed.
    async fn subscribe(&self) -> Self::SubscriptionType;
}

/// A subscription to a pipe
#[async_trait]
pub trait Subscription<T> {
    /// Wait for the next value from the entity
    ///
    /// This will return the new value.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    async fn recv(&mut self) -> Result<T, RecvError>;

    /// Get the next value but don't wait for it. Returns `None` if there is no value
    ///
    /// This will return the new value.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    fn try_recv(&mut self) -> Result<Option<T>, RecvError>;
}
