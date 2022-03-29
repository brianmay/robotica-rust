use std::time::Duration;

use tokio::time::{self, sleep_until, Interval};
use tokio::{select, sync::mpsc, time::Instant};

use crate::{send, spawn};

async fn maybe_sleep_until(instant: Option<Instant>) -> Option<()> {
    if let Some(instant) = instant {
        sleep_until(instant).await;
        Some(())
    } else {
        None
    }
}

pub fn delay_true(mut input: mpsc::Receiver<bool>, duration: Duration) -> mpsc::Receiver<bool> {
    let (tx, rx) = mpsc::channel(10);
    spawn(async move {
        let mut delay_until: Option<Instant> = None;

        loop {
            select! {
                Some(v) = input.recv() => {
                    if v && delay_until.is_none() {
                        delay_until = Some(Instant::now() + duration);
                    } else if !v {
                        delay_until = None;
                        send(&tx, v).await;
                    }
                },
                Some(()) = maybe_sleep_until(delay_until) => {
                    delay_until = None;
                    send(&tx, true).await
                },
                else => { break; }
            }
        }
    });

    rx
}

async fn maybe_tick(interval: &mut Option<Interval>) -> Option<()> {
    if let Some(interval) = interval {
        interval.tick().await;
        Some(())
    } else {
        None
    }
}

pub fn timer_true(mut input: mpsc::Receiver<bool>, duration: Duration) -> mpsc::Receiver<bool> {
    let (tx, rx) = mpsc::channel(10);
    spawn(async move {
        let mut interval: Option<Interval> = None;

        loop {
            select! {
                Some(v) = input.recv() => {
                    if v && interval.is_none() {
                        interval = Some(time::interval(duration));
                    } else if !v {
                        interval = None;
                        send(&tx, v).await;
                    }
                },
                Some(()) = maybe_tick(&mut interval) => {
                    send(&tx, true).await
                },
                else => { break; }
            }
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use tokio::time::timeout;

    use super::*;

    #[tokio::test]
    async fn test_delay_true() {
        let duration = Duration::from_millis(100);
        let wait_duration = Duration::from_millis(200);

        let (tx, rx) = mpsc::channel(10);
        let mut rx = delay_true(rx, duration);

        tx.send(false).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert!(!v);

        tx.send(true).await.unwrap();
        tx.send(false).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert!(!v);

        tx.send(true).await.unwrap();
        let v = timeout(wait_duration, rx.recv()).await;
        assert!(matches!(v, Ok(Some(true))));
    }

    #[tokio::test]
    async fn test_timer_true() {
        let duration = Duration::from_millis(100);
        let wait_duration = Duration::from_millis(200);

        let (tx, rx) = mpsc::channel(10);
        let mut rx = timer_true(rx, duration);

        tx.send(false).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert!(!v);

        tx.send(true).await.unwrap();
        tx.send(false).await.unwrap();
        // Note: Possible race condition, one true value could get sent before timer gets cancelled.
        let v = rx.recv().await.unwrap();
        assert!(!v);

        tx.send(true).await.unwrap();
        let v = timeout(wait_duration, rx.recv()).await;
        assert!(matches!(v, Ok(Some(true))));
        let v = timeout(wait_duration, rx.recv()).await;
        assert!(matches!(v, Ok(Some(true))));
    }
}
