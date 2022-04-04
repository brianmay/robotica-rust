use tokio::{sync::broadcast, sync::mpsc};

use crate::{send_or_panic, spawn, PIPE_SIZE};

pub struct Split<T: Send + Clone + 'static> {
    tx: broadcast::Sender<T>,
}

impl<T: Send + Clone + 'static> Split<T> {
    pub fn receiver(&self) -> mpsc::Receiver<T> {
        let (tx, rx) = mpsc::channel(PIPE_SIZE);

        let mut brx = self.tx.subscribe();
        spawn(async move {
            while let Ok(v) = brx.recv().await {
                send_or_panic(&tx, v).await;
            }
        });

        rx
    }
}

pub fn split<T: Send + Clone + 'static>(mut input: mpsc::Receiver<T>) -> Split<T> {
    let (tx, rx) = broadcast::channel(PIPE_SIZE);
    drop(rx);

    let tx_clone = tx.clone();
    spawn(async move {
        while let Some(v) = input.recv().await {
            tx_clone
                .send(v)
                .unwrap_or_else(|_| panic!("Cannot send value"));
        }
    });

    Split { tx }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_split2() {
        let (tx, rx) = mpsc::channel(10);
        let split = split(rx);
        let mut rx1 = split.receiver();
        let mut rx2 = split.receiver();

        tx.send(10).await.unwrap();
        let v = rx1.recv().await.unwrap();
        assert_eq!(v, 10);
        let v = rx2.recv().await.unwrap();
        assert_eq!(v, 10);

        tx.send(20).await.unwrap();
        let v = rx1.recv().await.unwrap();
        assert_eq!(v, 20);
        let v = rx2.recv().await.unwrap();
        assert_eq!(v, 20);
    }
}
