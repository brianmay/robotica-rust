use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time;

use crate::{send_and_wait, spawn, PIPE_SIZE};

pub fn timer(duration: Duration) -> mpsc::Receiver<bool> {
    let (tx, rx) = mpsc::channel(PIPE_SIZE);
    spawn(async move {
        let mut interval = time::interval(duration);

        loop {
            send_and_wait(&tx, true).await;
            interval.tick().await;
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use tokio::time::timeout;

    use super::*;

    #[tokio::test]
    async fn test_timer() {
        let duration = Duration::from_millis(100);
        let wait_duration = Duration::from_millis(200);

        let mut rx = timer(duration);

        let v = timeout(wait_duration, rx.recv()).await;
        assert!(matches!(v, Ok(Some(true))));
        let v = timeout(wait_duration, rx.recv()).await;
        assert!(matches!(v, Ok(Some(true))));
    }
}
