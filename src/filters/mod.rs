use std::{fmt::Debug, time::Duration};
use tokio::sync::mpsc::Receiver;

pub mod generic;
pub mod split;
pub mod teslamate;
pub mod timers;

pub trait ChainDiff<T> {
    fn diff(self) -> Receiver<(Option<T>, T)>;
    fn diff_with_initial_value(self, initial_value: Option<T>) -> Receiver<(Option<T>, T)>;
}

pub trait ChainChanged<T> {
    fn changed(self) -> Receiver<T>;
}

pub trait ChainDebug<T> {
    fn debug(self, msg: &str) -> Receiver<T>;
}

pub trait ChainGeneric<T> {
    fn map<U: Send + 'static>(self, callback: impl Send + 'static + Fn(T) -> U) -> Receiver<U>;
    fn map_with_state<U: Send + 'static, V: Send + 'static>(
        self,
        initial: V,
        callback: impl Send + 'static + Fn(&mut V, T) -> U,
    ) -> Receiver<U>;

    fn filter_map<U: Send + 'static>(
        self,
        callback: impl Send + 'static + Fn(T) -> Option<U>,
    ) -> Receiver<U>;
    fn filter(self, callback: impl Send + 'static + Fn(&T) -> bool) -> Receiver<T>;
    fn gate(self, gate: Receiver<bool>) -> Receiver<T>;
    fn startup_delay(self, duration: Duration, value: T) -> Receiver<T>;
}

pub trait ChainTimer {
    fn delay_true(self, duration: Duration) -> Receiver<bool>;
    fn delay_cancel(self, duration: Duration) -> Receiver<bool>;
    fn timer_true(self, duration: Duration) -> Receiver<bool>;
}

pub trait ChainSplit<T: Send + Clone + 'static> {
    fn split2(self) -> (Receiver<T>, Receiver<T>);
}

impl<T: Send + Clone + 'static> ChainDiff<T> for Receiver<T> {
    fn diff(self) -> Receiver<(Option<T>, T)> {
        generic::diff(self)
    }
    fn diff_with_initial_value(self, initial_value: Option<T>) -> Receiver<(Option<T>, T)> {
        generic::diff_with_initial_value(self, initial_value)
    }
}

impl<T: Send + Eq + 'static> ChainChanged<T> for Receiver<(Option<T>, T)> {
    fn changed(self) -> Receiver<T> {
        generic::changed(self)
    }
}

impl<T: Send + Debug + 'static> ChainDebug<T> for Receiver<T> {
    fn debug(self, msg: &str) -> Receiver<T> {
        generic::debug(self, msg)
    }
}

impl<T: Send + 'static> ChainGeneric<T> for Receiver<T> {
    fn map<U: Send + 'static>(self, callback: impl Send + 'static + Fn(T) -> U) -> Receiver<U> {
        generic::map(self, callback)
    }

    fn map_with_state<U: Send + 'static, V: Send + 'static>(
        self,
        initial: V,
        callback: impl Send + 'static + Fn(&mut V, T) -> U,
    ) -> Receiver<U> {
        generic::map_with_state(self, initial, callback)
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

    fn startup_delay(self, duration: Duration, value: T) -> Receiver<T> {
        timers::startup_delay(self, duration, value)
    }
}

impl ChainTimer for Receiver<bool> {
    fn delay_true(self, duration: Duration) -> Receiver<bool> {
        timers::delay_true(self, duration)
    }
    fn delay_cancel(self, duration: Duration) -> Receiver<bool> {
        timers::delay_cancel(self, duration)
    }

    fn timer_true(self, duration: Duration) -> Receiver<bool> {
        timers::timer_true(self, duration)
    }
}

impl<T: Send + Clone + 'static> ChainSplit<T> for Receiver<T> {
    fn split2(
        self,
    ) -> (
        tokio::sync::mpsc::Receiver<T>,
        tokio::sync::mpsc::Receiver<T>,
    ) {
        split::split2(self)
    }
}
