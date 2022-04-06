use std::time::Duration;

use tokio::time::{self, sleep_until, Interval};
use tokio::{select, sync::broadcast, time::Instant};

use crate::{recv, send_or_log, spawn, Pipe, RxPipe};

async fn maybe_sleep_until(instant: Option<Instant>) -> Option<()> {
    if let Some(instant) = instant {
        sleep_until(instant).await;
        Some(())
    } else {
        None
    }
}

fn delay_true(
    mut input: broadcast::Receiver<bool>,
    output: broadcast::Sender<bool>,
    duration: Duration,
) {
    spawn(async move {
        let mut delay_until: Option<Instant> = None;

        loop {
            select! {
                Ok(v) = recv(&mut input) => {
                    if v && delay_until.is_none() {
                        delay_until = Some(Instant::now() + duration);
                    } else if !v {
                        delay_until = None;
                        send_or_log(&output, v);
                    }
                },
                Some(()) = maybe_sleep_until(delay_until) => {
                    delay_until = None;
                    send_or_log(&output, true)
                },
                else => { break; }
            }
        }
    });
}

fn startup_delay<T: Send + Clone + 'static>(
    mut input: broadcast::Receiver<T>,
    output: broadcast::Sender<T>,
    duration: Duration,
    value: T,
) {
    spawn(async move {
        let mut delay_until: Option<Instant> = Some(Instant::now() + duration);
        let mut value = Some(value);

        loop {
            select! {
                Ok(v) = recv(&mut input) => {
                    delay_until = None;
                    send_or_log(&output, v);
                },
                Some(()) = maybe_sleep_until(delay_until) => {
                    delay_until = None;
                    if let Some(value) = value.take() {
                        send_or_log(&output, value);
                    }
                },
                else => { break; }
            }
        }
    });
}

fn delay_cancel(
    mut input: broadcast::Receiver<bool>,
    output: broadcast::Sender<bool>,
    duration: Duration,
) {
    spawn(async move {
        let mut delay_until: Option<Instant> = None;

        loop {
            select! {
                Ok(v) = recv(&mut input) => {
                    if v {
                        delay_until = Some(Instant::now() + duration);
                    } else {
                        delay_until = None;
                    }
                    send_or_log(&output, v);
                },
                Some(()) = maybe_sleep_until(delay_until) => {
                    delay_until = None;
                    send_or_log(&output, false)
                },
                else => { break; }
            }
        }
    });
}

async fn maybe_tick(interval: &mut Option<Interval>) -> Option<()> {
    if let Some(interval) = interval {
        interval.tick().await;
        Some(())
    } else {
        None
    }
}

fn timer_true(
    mut input: broadcast::Receiver<bool>,
    output: broadcast::Sender<bool>,
    duration: Duration,
) {
    spawn(async move {
        let mut interval: Option<Interval> = None;

        loop {
            select! {
                Ok(v) = recv(&mut input) => {
                    if v && interval.is_none() {
                        interval = Some(time::interval(duration));
                    } else if !v {
                        interval = None;
                        send_or_log(&output, v);
                    }
                },
                Some(()) = maybe_tick(&mut interval) => {
                    send_or_log(&output, true)
                },
                else => { break; }
            }
        }
    });
}

impl<T: Send + Clone + 'static> RxPipe<T> {
    pub fn startup_delay(&self, duration: Duration, value: T) -> RxPipe<T> {
        let output = Pipe::new();
        startup_delay(self.subscribe(), output.get_tx(), duration, value);
        output.to_rx_pipe()
    }
}

impl RxPipe<bool> {
    pub fn delay_true(&self, duration: Duration) -> RxPipe<bool> {
        let output = Pipe::new();
        delay_true(self.subscribe(), output.get_tx(), duration);
        output.to_rx_pipe()
    }
    pub fn delay_cancel(&self, duration: Duration) -> RxPipe<bool> {
        let output = Pipe::new();
        delay_cancel(self.subscribe(), output.get_tx(), duration);
        output.to_rx_pipe()
    }

    pub fn timer_true(&self, duration: Duration) -> RxPipe<bool> {
        let output = Pipe::new();
        timer_true(self.subscribe(), output.get_tx(), duration);
        output.to_rx_pipe()
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::sleep;

    use super::*;

    #[tokio::test]
    async fn test_delay_true() {
        let duration = Duration::from_millis(100);
        let wait_duration = Duration::from_millis(200);

        let (tx, in_rx) = broadcast::channel(10);
        let (out_tx, mut rx) = broadcast::channel(10);
        delay_true(in_rx, out_tx, duration);

        tx.send(false).unwrap();
        let v = rx.recv().await.unwrap();
        assert!(!v);

        tx.send(true).unwrap();
        tx.send(false).unwrap();
        let v = rx.recv().await.unwrap();
        assert!(!v);

        tx.send(true).unwrap();
        sleep(wait_duration).await;
        let v = rx.try_recv().unwrap();
        assert!(matches!(v, true));
    }

    #[tokio::test]
    async fn test_timer_true() {
        let duration = Duration::from_millis(100);
        let wait_duration = Duration::from_millis(200);

        let (tx, in_rx) = broadcast::channel(10);
        let (out_tx, mut rx) = broadcast::channel(10);
        timer_true(in_rx, out_tx, duration);

        tx.send(false).unwrap();
        let v = rx.recv().await.unwrap();
        assert!(!v);

        tx.send(true).unwrap();
        tx.send(false).unwrap();
        // FIXME: Possible race condition, one true value could get sent before timer gets cancelled.
        let v = rx.recv().await.unwrap();
        assert!(!v);

        tx.send(true).unwrap();
        sleep(wait_duration).await;
        let v = rx.try_recv().unwrap();
        assert!(matches!(v, true));

        sleep(wait_duration).await;
        let v = rx.try_recv().unwrap();
        assert!(matches!(v, true));
    }
}
