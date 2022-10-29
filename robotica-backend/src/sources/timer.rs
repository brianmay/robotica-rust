//! Sources that use timers to produce async data.
use std::time::Duration;
use tokio::time;
use tokio::time::Instant;

use crate::entities;
use crate::spawn;

/// Create a timer that sends outgoing messages at regularly spaced intervals.
#[must_use]
pub fn timer(duration: Duration, name: &str) -> entities::Receiver<Instant> {
    let (tx, rx) = entities::create_stateless_entity(name);

    spawn(async move {
        let mut interval = time::interval(duration);

        loop {
            let clone = Instant::now();
            tx.try_send(clone);
            interval.tick().await;
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use tokio::time::sleep;

    use super::*;

    #[tokio::test]
    async fn test_timer() {
        let duration = Duration::from_millis(100);
        let wait_duration = Duration::from_millis(200);

        let input = timer(duration, "test");
        let mut rx = input.subscribe().await;

        sleep(wait_duration).await;
        let _v = rx.try_recv().unwrap();
        // assert!(matches!(v, true));

        sleep(wait_duration).await;
        let _v = rx.try_recv().unwrap();
        // assert!(matches!(v, true));
    }
}
