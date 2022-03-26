pub mod filters;
pub mod sinks;
pub mod sources;

use tokio::sync::mpsc::Sender;

pub async fn send<T>(tx: &Sender<T>, data: T) {
    let a = tx.send(data).await;
    a.unwrap_or_else(|err| {
        panic!("send operation failred {err}");
    });
}
