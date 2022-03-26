use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time;

use crate::send;

pub fn timer(duration: Duration) -> mpsc::Receiver<bool> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        let mut interval = time::interval(duration);

        loop {
            send(&tx, true).await;
            interval.tick().await;
        }
    });

    rx
}
