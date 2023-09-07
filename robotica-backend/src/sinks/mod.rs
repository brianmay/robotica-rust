//! Sinks take one/more inputs and produce now outputs.
use crate::pipes::{Subscriber, Subscription};
use crate::spawn;

/// Create a sink that consumes incoming messages and discards them.
pub async fn null<T>(rx: impl Subscriber<T> + Send)
where
    T: Send + 'static,
{
    let s = rx.subscribe();
    let mut s = s.await;

    spawn(async move {
        while s.recv().await.is_ok() {
            // do nothing
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::pipes::stateless;

    use super::*;

    #[tokio::test]
    async fn test_null() {
        let (tx, rx) = stateless::create_pipe("test");
        null(rx).await;
        tx.try_send(10);
        tx.try_send(10);
        tx.try_send(10);
        tx.try_send(10);
    }
}
