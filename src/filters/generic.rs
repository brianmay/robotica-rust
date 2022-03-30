use crate::{send, spawn, PIPE_SIZE};
use log::*;
use tokio::{select, sync::mpsc};

pub fn changed<T: Send + Eq + 'static>(
    mut input: mpsc::Receiver<(Option<T>, T)>,
) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel(PIPE_SIZE);
    spawn(async move {
        while let Some(v) = input.recv().await {
            let v = match v {
                (None, _) => None,
                (Some(old), new) if old == new => None,
                (_, new) => Some(new),
            };
            if let Some(v) = v {
                send(&tx, v).await;
            }
        }
    });

    rx
}

pub fn diff<T: Send + Clone + 'static>(
    mut input: mpsc::Receiver<T>,
) -> mpsc::Receiver<(Option<T>, T)> {
    let (tx, rx) = mpsc::channel(PIPE_SIZE);
    spawn(async move {
        let mut old_value: Option<T> = None;
        while let Some(v) = input.recv().await {
            let v_clone = v.clone();
            send(&tx, (old_value, v_clone)).await;
            old_value = Some(v);
        }
    });

    rx
}

pub fn map<T: Send + 'static, U: Send + 'static>(
    mut input: mpsc::Receiver<T>,
    callback: impl Send + 'static + Fn(T) -> U,
) -> mpsc::Receiver<U> {
    let (tx, rx) = mpsc::channel(PIPE_SIZE);
    spawn(async move {
        while let Some(v) = input.recv().await {
            let v = callback(v);
            send(&tx, v).await;
        }
    });

    rx
}

pub fn map_with_state<T: Send + 'static, U: Send + 'static, V: Send + 'static>(
    mut input: mpsc::Receiver<T>,
    initial: V,
    callback: impl Send + 'static + Fn(&mut V, T) -> U,
) -> mpsc::Receiver<U> {
    let (tx, rx) = mpsc::channel(PIPE_SIZE);
    let mut state: V = initial;
    spawn(async move {
        while let Some(v) = input.recv().await {
            let v = callback(&mut state, v);
            send(&tx, v).await;
        }
    });

    rx
}

pub fn debug<T: Send + core::fmt::Debug + 'static>(
    mut input: mpsc::Receiver<T>,
    msg: &str,
) -> mpsc::Receiver<T> {
    let msg = msg.to_string();
    let (tx, rx) = mpsc::channel(PIPE_SIZE);
    spawn(async move {
        while let Some(v) = input.recv().await {
            debug!("debug {msg} {v:?}");
            send(&tx, v).await;
        }
    });

    rx
}

pub fn filter_map<T: Send + 'static, U: Send + 'static>(
    mut input: mpsc::Receiver<T>,
    callback: impl Send + 'static + Fn(T) -> Option<U>,
) -> mpsc::Receiver<U> {
    let (tx, rx) = mpsc::channel(PIPE_SIZE);
    spawn(async move {
        while let Some(v) = input.recv().await {
            let filter = callback(v);
            if let Some(v) = filter {
                send(&tx, v).await;
            }
        }
    });

    rx
}

pub fn filter<T: Send + 'static>(
    mut input: mpsc::Receiver<T>,
    callback: impl Send + 'static + Fn(&T) -> bool,
) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel(PIPE_SIZE);
    spawn(async move {
        while let Some(v) = input.recv().await {
            let filter = callback(&v);
            if filter {
                send(&tx, v).await;
            }
        }
    });

    rx
}

pub fn gate<T: Send + 'static>(
    mut input: mpsc::Receiver<T>,
    mut gate: mpsc::Receiver<bool>,
) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel(PIPE_SIZE);
    spawn(async move {
        let mut filter = true;
        loop {
            select! {
                Some(input) = input.recv() => {
                    if filter {
                        send(&tx, input).await;
                    }
                }
                Some(gate) = gate.recv() => {
                    filter = gate;
                }
                else => { break; }
            }
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_has_changed() {
        let (tx, rx) = mpsc::channel(10);
        let mut rx = diff(rx);

        tx.send(10).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, (None, 10));

        tx.send(10).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, (Some(10), 10));

        tx.send(20).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, (Some(10), 20));
    }

    #[tokio::test]
    async fn test_map() {
        let (tx, rx) = mpsc::channel(10);
        let mut rx = map(rx, |x| x + 1);

        tx.send(10).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 11);

        tx.send(20).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 21);
    }

    #[tokio::test]
    async fn test_map_with_state() {
        let (tx, rx) = mpsc::channel(10);
        let mut rx = map_with_state(rx, 3, |state, x| {
            *state += 1;
            x + *state
        });

        tx.send(10).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 14);

        tx.send(20).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 25);
    }

    #[tokio::test]
    async fn test_debug() {
        let (tx, rx) = mpsc::channel(10);
        let mut rx = debug(rx, "message");

        tx.send(10).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 10);

        tx.send(20).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 20);
    }

    #[tokio::test]
    async fn test_filter_map() {
        let (tx, rx) = mpsc::channel(10);
        let mut rx = filter_map(rx, |v| if v > 10 { Some(v + 1) } else { None });

        tx.send(10).await.unwrap();
        tx.send(10).await.unwrap();
        tx.send(20).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 21);
    }

    #[tokio::test]
    async fn test_filter() {
        let (tx, rx) = mpsc::channel(10);
        let mut rx = filter(rx, |&v| v > 10);

        tx.send(10).await.unwrap();
        tx.send(10).await.unwrap();
        tx.send(20).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 20);
    }

    #[tokio::test]
    async fn test_gate() {
        // FIXME: This test is awful
        // Sleep required to try to force gate to process messages in correct order.
        let (gate_tx, gate_rx) = mpsc::channel(10);

        let (tx, rx) = mpsc::channel(10);
        let mut rx = gate(rx, gate_rx);

        tx.send(10).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 10);

        gate_tx.send(false).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        tx.send(20).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        gate_tx.send(true).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        tx.send(30).await.unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 30);
    }
}
