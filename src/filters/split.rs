use tokio::sync::mpsc;

use crate::{send, spawn};

pub fn split2<T: Send + Clone + 'static>(
    mut input: mpsc::Receiver<T>,
) -> (mpsc::Receiver<T>, mpsc::Receiver<T>) {
    let (tx1, rx1) = mpsc::channel(10);
    let (tx2, rx2) = mpsc::channel(10);
    spawn(async move {
        while let Some(v) = input.recv().await {
            let clone = v.clone();
            send(&tx1, clone).await;
            send(&tx2, v).await;
        }
    });

    (rx1, rx2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_split2() {
        let (tx, rx) = mpsc::channel(10);
        let (mut rx1, mut rx2) = split2(rx);

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
