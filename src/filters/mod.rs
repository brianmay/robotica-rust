use std::{fmt::Debug, time::Duration};

use crate::{Pipe, RxPipe, TxPipe};

mod generic;
mod teslamate;
mod timers;

impl<T: Send + Clone + 'static> RxPipe<T> {
    pub fn diff(&self) -> RxPipe<(Option<T>, T)> {
        let output = Pipe::new();
        generic::diff(self.subscribe(), output.get_tx());
        output.to_rx_pipe()
    }
    pub fn diff_with_initial_value(&self, initial_value: Option<T>) -> RxPipe<(Option<T>, T)> {
        let output = Pipe::new();
        generic::diff_with_initial_value(self.subscribe(), output.get_tx(), initial_value);
        output.to_rx_pipe()
    }
}

impl<T: Send + Eq + Clone + 'static> RxPipe<(Option<T>, T)> {
    pub fn changed(&self) -> RxPipe<T> {
        let output = Pipe::new();
        generic::changed(self.subscribe(), output.get_tx());
        output.to_rx_pipe()
    }
}

impl<T: Send + Debug + Clone + 'static> RxPipe<T> {
    pub fn debug(&self, msg: &str) -> RxPipe<T> {
        let output = Pipe::new();
        generic::debug(self.subscribe(), output.get_tx(), msg);
        output.to_rx_pipe()
    }
}

impl<T: Send + Clone + 'static> RxPipe<T> {
    pub fn map<U: Send + Clone + 'static>(
        &self,
        callback: impl Send + 'static + Fn(T) -> U,
    ) -> RxPipe<U> {
        let output = Pipe::new();
        generic::map(self.subscribe(), output.get_tx(), callback);
        output.to_rx_pipe()
    }

    pub fn map_with_state<U: Send + Clone + 'static, V: Send + 'static>(
        &self,
        initial: V,
        callback: impl Send + 'static + Fn(&mut V, T) -> U,
    ) -> RxPipe<U> {
        let output = Pipe::new();
        generic::map_with_state(self.subscribe(), output.get_tx(), initial, callback);
        output.to_rx_pipe()
    }

    pub fn filter_map<U: Send + Clone + 'static>(
        &self,
        callback: impl Send + 'static + Fn(T) -> Option<U>,
    ) -> RxPipe<U> {
        let output = Pipe::new();
        generic::filter_map(self.subscribe(), output.get_tx(), callback);
        output.to_rx_pipe()
    }

    pub fn filter(&self, callback: impl Send + 'static + Fn(&T) -> bool) -> RxPipe<T> {
        let output = Pipe::new();
        generic::filter(self.subscribe(), output.get_tx(), callback);
        output.to_rx_pipe()
    }

    pub fn gate(&self, gate: RxPipe<bool>) -> RxPipe<T> {
        let output = Pipe::new();
        generic::gate(self.subscribe(), gate.subscribe(), output.get_tx());
        output.to_rx_pipe()
    }

    pub fn startup_delay(&self, duration: Duration, value: T) -> RxPipe<T> {
        let output = Pipe::new();
        timers::startup_delay(self.subscribe(), output.get_tx(), duration, value);
        output.to_rx_pipe()
    }

    pub fn copy_to(&self, output: TxPipe<T>) {
        generic::copy(self.subscribe(), output.get_tx());
    }
}

impl RxPipe<bool> {
    pub fn delay_true(&self, duration: Duration) -> RxPipe<bool> {
        let output = Pipe::new();
        timers::delay_true(self.subscribe(), output.get_tx(), duration);
        output.to_rx_pipe()
    }
    pub fn delay_cancel(&self, duration: Duration) -> RxPipe<bool> {
        let output = Pipe::new();
        timers::delay_cancel(self.subscribe(), output.get_tx(), duration);
        output.to_rx_pipe()
    }

    pub fn timer_true(&self, duration: Duration) -> RxPipe<bool> {
        let output = Pipe::new();
        timers::timer_true(self.subscribe(), output.get_tx(), duration);
        output.to_rx_pipe()
    }
}

pub fn requires_plugin(
    battery_level: RxPipe<usize>,
    plugged_in: RxPipe<bool>,
    geofence: RxPipe<String>,
    reminder: RxPipe<bool>,
) -> RxPipe<bool> {
    let output = Pipe::new();
    teslamate::requires_plugin(
        battery_level.subscribe(),
        plugged_in.subscribe(),
        geofence.subscribe(),
        reminder.subscribe(),
        output.get_tx(),
    );
    output.to_rx_pipe()
}

pub fn is_insecure(is_user_present: RxPipe<bool>, locked: RxPipe<bool>) -> RxPipe<bool> {
    let output = Pipe::new();
    teslamate::is_insecure(
        is_user_present.subscribe(),
        locked.subscribe(),
        output.get_tx(),
    );
    output.to_rx_pipe()
}
