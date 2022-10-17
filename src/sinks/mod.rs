//! Sinks take one/more inputs and produce now outputs.
use crate::{entities, spawn};

/// Create a sink that consumes incoming messages and discards them.
pub async fn null<T: Send + Clone + 'static>(rx: entities::Receiver<T>) {
    let mut s = rx.subscribe().await;
    spawn(async move {
        while s.recv().await.is_ok() {
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
