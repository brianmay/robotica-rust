use tokio::sync::broadcast;

use crate::{recv, spawn, RxPipe};

fn null<T: Send + Clone + 'static>(mut input: broadcast::Receiver<T>) {
    spawn(async move {
        while (recv(&mut input).await).is_ok() {
            // do nothing
        }
    });
}

impl<T: Send + Clone + 'static> RxPipe<T> {
    pub fn null(&self) {
        null(self.subscribe());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_null() {
        let (tx, rx) = broadcast::channel(1);
        null(rx);
        tx.send(10).unwrap();
        tx.send(10).unwrap();
        tx.send(10).unwrap();
        tx.send(10).unwrap();
    }
}
