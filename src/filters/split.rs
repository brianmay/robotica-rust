use tokio::sync::mpsc;

use crate::send;

pub fn split2<T: Send + Clone + 'static>(
    mut input: mpsc::Receiver<T>,
) -> (mpsc::Receiver<T>, mpsc::Receiver<T>) {
    let (tx1, rx1) = mpsc::channel(10);
    let (tx2, rx2) = mpsc::channel(10);
    tokio::spawn(async move {
        while let Some(v) = input.recv().await {
            let clone = v.clone();
            send(&tx1, clone).await;
            send(&tx2, v).await;
        }
    });

    (rx1, rx2)
}
