use log::*;
use tokio::sync::mpsc;

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
                    let a = tx.send((prev, v_clone)).await;
                    a.unwrap_or_else(|err| {
                        error!("send operation failed {err}");
                    });
                }
            };
            old_value = Some(v);
        }
    });

    rx
}

pub fn map<T: Send + core::fmt::Debug + 'static, U: Send + core::fmt::Debug + 'static>(
    mut input: mpsc::Receiver<T>,
    callback: fn(T) -> U,
) -> mpsc::Receiver<U> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        while let Some(v) = input.recv().await {
            println!("map {v:?}");
            let v = callback(v);
            println!("--> {v:?}");
            tx.send(v).await.unwrap();
        }
    });

    rx
}

pub fn filter_map<T: Send + core::fmt::Debug + 'static, U: Send + core::fmt::Debug + 'static>(
    mut input: mpsc::Receiver<T>,
    callback: fn(T) -> Option<U>,
) -> mpsc::Receiver<U> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        while let Some(v) = input.recv().await {
            println!("filter_map {v:?}");
            if let Some(v) = callback(v) {
                println!("--> {v:?}");
                tx.send(v).await.unwrap();
            }
        }
    });

    rx
}

pub fn filter<T: Send + core::fmt::Debug + 'static>(
    mut input: mpsc::Receiver<T>,
    callback: fn(&T) -> bool,
) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        while let Some(v) = input.recv().await {
            let filter = callback(&v);
            if filter {
                tx.send(v).await.unwrap();
            }
        }
    });

    rx
}
