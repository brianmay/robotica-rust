use std::time::Duration;

use tokio::time::{self, sleep_until, Interval};
use tokio::{select, sync::mpsc, time::Instant};

use crate::send;

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
    tokio::spawn(async move {
        let mut delay_until: Option<Instant> = None;

        loop {
            select! {
                Some(v) = input.recv() => {
                    if v {
                        delay_until = Some(Instant::now() + duration);
                    } else {
                        delay_until = None;
                        send(&tx, v).await;
                    }
                },
                Some(())= maybe_sleep_until(delay_until) => {
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
    tokio::spawn(async move {
        let mut interval: Option<Interval> = None;

        loop {
            select! {
                Some(v) = input.recv() => {
                    if v {
                        interval = Some(time::interval(duration));
                    } else {
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
