//! Sources that use timers to produce async data.
use std::time::Duration;

use tokio::time;

use crate::{send_or_log, spawn, Pipe, RxPipe};

/// Create a timer that sends outgoing messages at regularly spaced intervals.
pub fn timer<T: Clone + Send + 'static>(duration: Duration, value: T) -> RxPipe<T> {
    let output = Pipe::new();
    let tx = output.get_tx();

    spawn(async move {
        let mut interval = time::interval(duration);

        loop {
            send_or_log(&tx, value.clone());
            interval.tick().await;
        }
    });

    output.to_rx_pipe()
}

#[cfg(test)]
mod tests {
    use tokio::time::sleep;

    use super::*;

    #[tokio::test]
    async fn test_timer() {
        let duration = Duration::from_millis(100);
        let wait_duration = Duration::from_millis(200);

        let input = timer(duration, true);
        let mut rx = input.subscribe();

        sleep(wait_duration).await;
        let v = rx.try_recv().unwrap();
        assert!(matches!(v, true));

        sleep(wait_duration).await;
        let v = rx.try_recv().unwrap();
        assert!(matches!(v, true));
    }
}
