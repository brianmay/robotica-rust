use tokio::{select, sync::mpsc};

use crate::send;

pub fn has_changed<T: Send + Eq + Clone + 'static>(
    mut input: mpsc::Receiver<T>,
) -> mpsc::Receiver<(T, T)> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        let mut old_value: Option<T> = None;
        while let Some(v) = input.recv().await {
            if let Some(prev) = old_value {
                if prev != v {
                    let v_clone = v.clone();
                    send(&tx, (prev, v_clone)).await;
                }
            };
            old_value = Some(v);
        }
    });

    rx
}

pub fn map<T: Send + 'static, U: Send + 'static>(
    mut input: mpsc::Receiver<T>,
    callback: impl Send + 'static + Fn(T) -> U,
) -> mpsc::Receiver<U> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        while let Some(v) = input.recv().await {
            let v = callback(v);
            send(&tx, v).await;
        }
    });

    rx
}

pub fn debug<T: Send + core::fmt::Debug + 'static>(
    mut input: mpsc::Receiver<T>,
    msg: String,
) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        while let Some(v) = input.recv().await {
            println!("debug {msg} {v:?}");
            send(&tx, v).await;
        }
    });

    rx
}

pub fn filter_map<T: Send + 'static, U: Send + 'static>(
    mut input: mpsc::Receiver<T>,
    callback: impl Send + 'static + Fn(T) -> Option<U>,
) -> mpsc::Receiver<U> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
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
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
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
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        let mut filter = true;
        loop {
            select! {
                input = input.recv() => {
                    if let Some(input) = input {
                        if filter {
                            send(&tx, input).await;
                        }
                    } else {
                        break;
                    }

                }
                gate = gate.recv() => {
                    if let Some(gate) = gate {
                        filter = gate;
                    }
                }
            }
        }
    });

    rx
}
