//! Sinks take one/more inputs and produce now outputs.
use crate::{entities, spawn};

/// Create a sink that consumes incoming messages and discards them.
pub async fn null<T>(rx: entities::Receiver<T>)
where
    T: entities::Data + Send + 'static,
    T::Received: Send + 'static,
    T::Sent: Send + 'static,
{
    let mut s = rx.subscribe().await;
    spawn(async move {
        while s.recv_value().await.is_ok() {
            // do nothing
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_null() {
        let (tx, rx) = entities::create_stateless_entity("test");
        null(rx).await;
        tx.try_send(10);
        tx.try_send(10);
        tx.try_send(10);
        tx.try_send(10);
    }
}
