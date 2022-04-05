use std::time::Duration;

use tokio::time;

use crate::{send_or_discard, spawn, Pipe};

pub fn timer(duration: Duration) -> Pipe<bool> {
    let output = Pipe::new();
    let tx = output.get_tx();

    spawn(async move {
        let mut interval = time::interval(duration);

        loop {
            send_or_discard(&tx, true);
            interval.tick().await;
        }
    });

    output
}

#[cfg(test)]
mod tests {
    use tokio::time::sleep;

    use super::*;

    #[tokio::test]
    async fn test_timer() {
        let duration = Duration::from_millis(100);
        let wait_duration = Duration::from_millis(200);

        let input = timer(duration);
        let mut rx = input.subscribe();

        sleep(wait_duration).await;
        let v = rx.try_recv().unwrap();
        assert!(matches!(v, true));

        sleep(wait_duration).await;
        let v = rx.try_recv().unwrap();
        assert!(matches!(v, true));
    }
}
