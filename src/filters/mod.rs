use std::{fmt::Debug, time::Duration};
use tokio::sync::mpsc::Receiver;

pub mod generic;
pub mod teslamate;
pub mod timers;

pub trait ChainGeneric<T> {
    fn has_changed(self) -> Receiver<(T, T)>;
    fn map<U: Send + 'static>(self, callback: impl Send + 'static + Fn(T) -> U) -> Receiver<U>;
    fn debug(self, msg: String) -> Receiver<T>;
    fn filter_map<U: Send + 'static>(
        self,
        callback: impl Send + 'static + Fn(T) -> Option<U>,
    ) -> Receiver<U>;
    fn filter(self, callback: impl Send + 'static + Fn(&T) -> bool) -> Receiver<T>;
    fn gate(self, gate: Receiver<bool>) -> Receiver<T>;
}

pub trait ChainTimer {
    fn delay_true(self, duration: Duration) -> Receiver<bool>;
    fn timer(self, duration: Duration) -> Receiver<bool>;
}

impl<T: Send + Eq + Debug + Clone + 'static> ChainGeneric<T> for Receiver<T> {
    fn has_changed(self) -> Receiver<(T, T)> {
        generic::has_changed(self)
    }

    fn map<U: Send + 'static>(self, callback: impl Send + 'static + Fn(T) -> U) -> Receiver<U> {
        generic::map(self, callback)
    }

    fn debug(self, msg: String) -> Receiver<T> {
        generic::debug(self, msg)
    }

    fn filter_map<U: Send + 'static>(
        self,
        callback: impl Send + 'static + Fn(T) -> Option<U>,
    ) -> Receiver<U> {
        generic::filter_map(self, callback)
    }

    fn filter(self, callback: impl Send + 'static + Fn(&T) -> bool) -> Receiver<T> {
        generic::filter(self, callback)
    }

    fn gate(self, gate: Receiver<bool>) -> Receiver<T> {
        generic::gate(self, gate)
    }
}

impl ChainTimer for Receiver<bool> {
    fn delay_true(self, duration: Duration) -> Receiver<bool> {
        timers::delay_true(self, duration)
    }

    fn timer(self, duration: Duration) -> Receiver<bool> {
        timers::timer(self, duration)
    }
}
