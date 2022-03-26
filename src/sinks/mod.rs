use tokio::sync::mpsc::{self, Receiver};

pub fn null<T: Send + 'static>(mut input: mpsc::Receiver<T>) -> mpsc::Receiver<T> {
    let (_tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
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
