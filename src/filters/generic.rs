//! Generic filter functions
use crate::{recv, send_or_log, spawn, Pipe, RxPipe, TxPipe};
use log::*;
use std::fmt::Debug;
use tokio::{select, sync::broadcast};

fn changed<T: Send + Eq + Clone + 'static>(
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

fn changed_or_unknown<T: Send + Eq + Clone + 'static>(
    mut input: broadcast::Receiver<(Option<T>, T)>,
    output: broadcast::Sender<T>,
) {
    spawn(async move {
        while let Ok(v) = recv(&mut input).await {
            let v = match v {
                (None, new) => Some(new),
                (Some(old), new) if old == new => None,
                (_, new) => Some(new),
            };
            if let Some(v) = v {
                send_or_log(&output, v);
            }
        }
    });
}

fn diff<T: Send + Clone + 'static>(
    input: broadcast::Receiver<T>,
    output: broadcast::Sender<(Option<T>, T)>,
) {
    diff_with_initial_value(input, output, None)
}

fn diff_with_initial_value<T: Send + Clone + 'static>(
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

fn map<T: Send + Clone + 'static, U: Send + 'static>(
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

fn map_with_state<T: Send + Clone + 'static, U: Send + 'static, V: Send + 'static>(
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

fn debug<T: Send + Clone + core::fmt::Debug + 'static>(
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

fn filter_map<T: Send + Clone + 'static, U: Send + 'static>(
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

fn copy<T: Send + Clone + 'static>(
    mut input: broadcast::Receiver<T>,
    output: broadcast::Sender<T>,
) {
    spawn(async move {
        while let Ok(v) = recv(&mut input).await {
            send_or_log(&output, v);
        }
    });
}

fn filter<T: Send + Clone + 'static>(
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

fn gate<T: Send + Clone + 'static>(
    mut input: broadcast::Receiver<T>,
    mut gate: broadcast::Receiver<bool>,
    output: broadcast::Sender<T>,
) {
    spawn(async move {
        let mut filter = true;
        loop {
            select! {
                biased;

                Ok(gate) = recv(&mut gate) => {
                    filter = gate;
                }
                Ok(input) = recv(&mut input) => {
                    if filter {
                        send_or_log(&output, input);
                    }
                }
                else => { break; }
            }
        }
    });
}

fn _if_else<T: Send + Clone + 'static>(
    mut gate: broadcast::Receiver<bool>,
    mut if_true: broadcast::Receiver<T>,
    mut if_false: broadcast::Receiver<T>,
    output: broadcast::Sender<T>,
) {
    spawn(async move {
        let mut filter: Option<bool> = None;
        let mut true_value: Option<T> = None;
        let mut false_value: Option<T> = None;

        loop {
            select! {
                Ok(gate) = recv(&mut gate) => {
                    filter = Some(gate);
                }
                Ok(input) = recv(&mut if_true) => {
                    true_value = Some(input);
                }
                Ok(input) = recv(&mut if_false) => {
                    false_value = Some(input);
                }
                else => { break; }
            }
            let value = match filter {
                Some(true) => &true_value,
                Some(false) => &false_value,
                None => &None,
            };
            if let Some(v) = value {
                send_or_log(&output, v.clone());
            }
        }
    });
}

/// Pass through if_true if gate value is true, otherwise pass through if_false
pub fn if_else<T: Send + Clone + 'static>(
    gate: RxPipe<bool>,
    if_true: RxPipe<T>,
    if_false: RxPipe<T>,
) -> RxPipe<T> {
    let output = Pipe::new();
    _if_else(
        gate.subscribe(),
        if_true.subscribe(),
        if_false.subscribe(),
        output.get_tx(),
    );
    output.to_rx_pipe()
}

impl<T: Send + Clone + 'static> RxPipe<T> {
    /// Add previous value to the input stream.
    ///
    /// If there was no previous value, then add None.
    pub fn diff(&mut self) -> RxPipe<(Option<T>, T)> {
        let output = Pipe::new();
        diff(self.subscribe(), output.get_tx());
        output.to_rx_pipe()
    }

    /// Add previous value to the input stream.
    ///
    /// If there was no previous value, then add the initial value.
    pub fn diff_with_initial_value(&mut self, initial_value: Option<T>) -> RxPipe<(Option<T>, T)> {
        let output = Pipe::new();
        diff_with_initial_value(self.subscribe(), output.get_tx(), initial_value);
        output.to_rx_pipe()
    }
}

impl<T: Send + Eq + Clone + 'static> RxPipe<(Option<T>, T)> {
    /// Has the stream from [Self::diff] or [Self::diff_with_initial_value] changed?
    pub fn changed(&self) -> RxPipe<T> {
        let output = Pipe::new();
        changed(self.subscribe(), output.get_tx());
        output.to_rx_pipe()
    }

    /// Has the stream from [Self::diff] or [Self::diff_with_initial_value] changed or was previous value unknown?
    pub fn changed_or_unknown(&self) -> RxPipe<T> {
        let output = Pipe::new();
        changed_or_unknown(self.subscribe(), output.get_tx());
        output.to_rx_pipe()
    }
}

impl<T: Send + Debug + Clone + 'static> RxPipe<T> {
    /// Log the value and pass through unchanged.
    pub fn debug(&self, msg: &str) -> RxPipe<T> {
        let output = Pipe::new();
        debug(self.subscribe(), output.get_tx(), msg);
        output.to_rx_pipe()
    }
}

impl<T: Send + Clone + 'static> RxPipe<T> {
    /// Map value through function and (optionally) change its type.
    pub fn map<U: Send + Clone + 'static>(
        &self,
        callback: impl Send + 'static + Fn(T) -> U,
    ) -> RxPipe<U> {
        let output = Pipe::new();
        map(self.subscribe(), output.get_tx(), callback);
        output.to_rx_pipe()
    }

    /// Map value through function and optionally change its type and keep track of state.
    ///
    /// Unlike with [Self::map], the function passes a mutable state variable, which is
    /// preserved between calls. The initial value of the state is passed as a parameter.
    pub fn map_with_state<U: Send + Clone + 'static, V: Send + 'static>(
        &self,
        initial: V,
        callback: impl Send + 'static + Fn(&mut V, T) -> U,
    ) -> RxPipe<U> {
        let output = Pipe::new();
        map_with_state(self.subscribe(), output.get_tx(), initial, callback);
        output.to_rx_pipe()
    }

    /// Map value through function and optionally discard
    ///
    /// Unlike with [Self::map], if the function returns None, the value is dropped.
    pub fn filter_map<U: Send + Clone + 'static>(
        &self,
        callback: impl Send + 'static + Fn(T) -> Option<U>,
    ) -> RxPipe<U> {
        let output = Pipe::new();
        filter_map(self.subscribe(), output.get_tx(), callback);
        output.to_rx_pipe()
    }

    /// Filter value through function and optionally discard
    ///
    /// This function always returns the same data. If the function returns True the value is transmitted,
    /// otherwise the value is dropped.
    pub fn filter(&self, callback: impl Send + 'static + Fn(&T) -> bool) -> RxPipe<T> {
        let output = Pipe::new();
        filter(self.subscribe(), output.get_tx(), callback);
        output.to_rx_pipe()
    }

    /// Filter value and optionally discard
    ///
    /// Similar to [Self::filter], but the allow value comes from an RxPipe.
    pub fn gate(&self, allow: RxPipe<bool>) -> RxPipe<T> {
        let output = Pipe::new();
        gate(self.subscribe(), allow.subscribe(), output.get_tx());
        output.to_rx_pipe()
    }

    /// Pass all values on to a [TxPipe].
    pub fn copy_to(&self, output: &TxPipe<T>) {
        copy(self.subscribe(), output.get_tx());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_diff() {
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
    async fn test_changed() {
        let (tx, in_rx) = broadcast::channel(10);
        let (out_tx, mut rx) = broadcast::channel(10);
        changed(in_rx, out_tx);

        tx.send((None, 10)).unwrap();

        tx.send((Some(10), 20)).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 20);

        tx.send((Some(20), 20)).unwrap();

        tx.send((Some(20), 30)).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 30);
    }

    #[tokio::test]
    async fn test_changed_or_unknown() {
        let (tx, in_rx) = broadcast::channel(10);
        let (out_tx, mut rx) = broadcast::channel(10);
        changed_or_unknown(in_rx, out_tx);

        tx.send((None, 10)).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 10);

        tx.send((Some(10), 20)).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 20);

        tx.send((Some(20), 20)).unwrap();

        tx.send((Some(20), 30)).unwrap();
        let v = rx.recv().await.unwrap();
        assert_eq!(v, 30);
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
