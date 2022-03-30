use tokio::sync::mpsc::{self, Receiver};

use crate::{spawn, PIPE_SIZE};

pub fn null<T: Send + 'static>(mut input: mpsc::Receiver<T>) -> mpsc::Receiver<T> {
    let (_tx, rx) = mpsc::channel(PIPE_SIZE);
    spawn(async move {
        while (input.recv().await).is_some() {
            // do nothing
        }
    });

    rx
}

pub trait ChainNull<T: Send + Clone + 'static> {
    fn null(self);
}

impl<T: Send + Clone + 'static> ChainNull<T> for Receiver<T> {
    fn null(self) {
        null(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_null() {
        let (tx, rx) = mpsc::channel(1);
        null(rx);
        tx.send(10).await.unwrap();
        tx.send(10).await.unwrap();
        tx.send(10).await.unwrap();
        tx.send(10).await.unwrap();
    }
}
