use crate::{recv, send_or_log, spawn};
use log::*;
use tokio::{select, sync::broadcast};

pub fn changed<T: Send + Eq + Clone + 'static>(
    mut input: broadcast::Receiver<(Option<T>, T)>,
    output: broadcast::Sender<T>,
) {
    spawn(async move {
        while let Ok(v) = recv(&mut input).await {
            let v = match v {
                (None, _) => None,
                (Some(old), new) if old == new => None,
                (_, new) => Some(new),
            };
            if let Some(v) = v {
                send_or_log(&output, v);
            }
        }
    });
}

pub fn diff<T: Send + Clone + 'static>(
    input: broadcast::Receiver<T>,
    output: broadcast::Sender<(Option<T>, T)>,
) {
    diff_with_initial_value(input, output, None)
}

pub fn diff_with_initial_value<T: Send + Clone + 'static>(
    mut input: broadcast::Receiver<T>,
    output: broadcast::Sender<(Option<T>, T)>,
    initial_value: Option<T>,
) {
    spawn(async move {
        let mut old_value = initial_value;
        while let Ok(v) = recv(&mut input).await {
            let v_clone = v.clone();
            send_or_log(&output, (old_value, v_clone));
            old_value = Some(v);
        }
    });
}

pub fn map<T: Send + Clone + 'static, U: Send + 'static>(
    mut input: broadcast::Receiver<T>,
    output: broadcast::Sender<U>,
    callback: impl Send + 'static + Fn(T) -> U,
) {
    spawn(async move {
        while let Ok(v) = recv(&mut input).await {
            let v = callback(v);
            send_or_log(&output, v);
        }
    });
}

pub fn map_with_state<T: Send + Clone + 'static, U: Send + 'static, V: Send + 'static>(
    mut input: broadcast::Receiver<T>,
    output: broadcast::Sender<U>,
    initial: V,
    callback: impl Send + 'static + Fn(&mut V, T) -> U,
) {
    let mut state: V = initial;
    spawn(async move {
        while let Ok(v) = recv(&mut input).await {
            let v = callback(&mut state, v);
            send_or_log(&output, v);
        }
    });
}

pub fn debug<T: Send + Clone + core::fmt::Debug + 'static>(
    mut input: broadcast::Receiver<T>,
    output: broadcast::Sender<T>,
    msg: &str,
) {
    let msg = msg.to_string();
    spawn(async move {
        while let Ok(v) = recv(&mut input).await {
            debug!("debug {msg} {v:?}");
            send_or_log(&output, v);
        }
    });
}

pub fn filter_map<T: Send + Clone + 'static, U: Send + 'static>(
    mut input: broadcast::Receiver<T>,
    output: broadcast::Sender<U>,
    callback: impl Send + 'static + Fn(T) -> Option<U>,
) {
    spawn(async move {
        while let Ok(v) = recv(&mut input).await {
            let filter = callback(v);
            if let Some(v) = filter {
                send_or_log(&output, v);
            }
        }
    });
}

pub fn copy<T: Send + Clone + 'static>(
    mut input: broadcast::Receiver<T>,
    output: broadcast::Sender<T>,
) {
    spawn(async move {
        while let Ok(v) = recv(&mut input).await {
            send_or_log(&output, v);
        }
    });
}

pub fn filter<T: Send + Clone + 'static>(
    mut input: broadcast::Receiver<T>,
    output: broadcast::Sender<T>,
    callback: impl Send + 'static + Fn(&T) -> bool,
) {
    spawn(async move {
        while let Ok(v) = recv(&mut input).await {
            let filter = callback(&v);
            if filter {
                send_or_log(&output, v);
            }
        }
    });
}

pub fn gate<T: Send + Clone + 'static>(
    mut input: broadcast::Receiver<T>,
    mut gate: broadcast::Receiver<bool>,
    output: broadcast::Sender<T>,
) {
    spawn(async move {
        let mut filter = true;
        loop {
            select! {
                Ok(input) = recv(&mut input) => {
                    if filter {
                        send_or_log(&output, input);
                    }
                }
                Ok(gate) = recv(&mut gate) => {
                    filter = gate;
                }
                else => { break; }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_has_changed() {
        let (tx, in_rx) = broadcast::channel(10);
        let (out_tx, mut rx) = broadcast::channel(10);
        diff(in_rx, out_tx);

        tx.send(10).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, (None, 10));

        tx.send(10).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, (Some(10), 10));

        tx.send(20).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, (Some(10), 20));
    }

    #[tokio::test]
    async fn test_map() {
        let (tx, in_rx) = broadcast::channel(10);
        let (out_tx, mut rx) = broadcast::channel(10);
        map(in_rx, out_tx, |x| x + 1);

        tx.send(10).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 11);

        tx.send(20).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 21);
    }

    #[tokio::test]
    async fn test_map_with_state() {
        let (tx, in_rx) = broadcast::channel(10);
        let (out_tx, mut rx) = broadcast::channel(10);
        map_with_state(in_rx, out_tx, 3, |state, x| {
            *state += 1;
            x + *state
        });

        tx.send(10).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 14);

        tx.send(20).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 25);
    }

    #[tokio::test]
    async fn test_debug() {
        let (tx, in_rx) = broadcast::channel(10);
        let (out_tx, mut rx) = broadcast::channel(10);
        debug(in_rx, out_tx, "message");

        tx.send(10).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 10);

        tx.send(20).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 20);
    }

    #[tokio::test]
    async fn test_filter_map() {
        let (tx, in_rx) = broadcast::channel(10);
        let (out_tx, mut rx) = broadcast::channel(10);
        filter_map(in_rx, out_tx, |v| if v > 10 { Some(v + 1) } else { None });

        tx.send(10).unwrap();
        tx.send(10).unwrap();
        tx.send(20).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 21);
    }

    #[tokio::test]
    async fn test_copy() {
        let (tx, in_rx) = broadcast::channel(10);
        let (out_tx, mut rx) = broadcast::channel(10);
        copy(in_rx, out_tx);

        tx.send(10).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 10);

        tx.send(10).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 10);

        tx.send(20).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 20);
    }

    #[tokio::test]
    async fn test_filter() {
        let (tx, in_rx) = broadcast::channel(10);
        let (out_tx, mut rx) = broadcast::channel(10);
        filter(in_rx, out_tx, |&v| v > 10);

        tx.send(10).unwrap();
        tx.send(10).unwrap();
        tx.send(20).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 20);
    }

    #[tokio::test]
    async fn test_gate() {
        // FIXME: This test is awful
        // Sleep required to try to force gate to process messages in correct order.
        let (gate_tx, gate_rx) = broadcast::channel(10);

        let (tx, in_rx) = broadcast::channel(10);
        let (out_tx, mut rx) = broadcast::channel(10);

        gate(in_rx, gate_rx, out_tx);

        tx.send(10).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 10);

        gate_tx.send(false).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        tx.send(20).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        gate_tx.send(true).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        tx.send(30).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 30);
    }
}
