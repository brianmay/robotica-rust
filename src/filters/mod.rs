use std::{fmt::Debug, time::Duration};

use crate::Pipe;

mod generic;
mod teslamate;
mod timers;

impl<T: Send + Clone + 'static> Pipe<T> {
    pub fn diff(&self) -> Pipe<(Option<T>, T)> {
        let output = Pipe::new();
        generic::diff(self.subscribe(), output.get_tx());
        output
    }
    pub fn diff_with_initial_value(&self, initial_value: Option<T>) -> Pipe<(Option<T>, T)> {
        let output = Pipe::new();
        generic::diff_with_initial_value(self.subscribe(), output.get_tx(), initial_value);
        output
    }
}

impl<T: Send + Eq + Clone + 'static> Pipe<(Option<T>, T)> {
    pub fn changed(&self) -> Pipe<T> {
        let output = Pipe::new();
        generic::changed(self.subscribe(), output.get_tx());
        output
    }
}

impl<T: Send + Debug + Clone + 'static> Pipe<T> {
    pub fn debug(&self, msg: &str) -> Pipe<T> {
        let output = Pipe::new();
        generic::debug(self.subscribe(), output.get_tx(), msg);
        output
    }
}

impl<T: Send + Clone + 'static> Pipe<T> {
    pub fn map<U: Send + Clone + 'static>(
        &self,
        callback: impl Send + 'static + Fn(T) -> U,
    ) -> Pipe<U> {
        let output = Pipe::new();
        generic::map(self.subscribe(), output.get_tx(), callback);
        output
    }

    pub fn map_with_state<U: Send + Clone + 'static, V: Send + 'static>(
        &self,
        initial: V,
        callback: impl Send + 'static + Fn(&mut V, T) -> U,
    ) -> Pipe<U> {
        let output = Pipe::new();
        generic::map_with_state(self.subscribe(), output.get_tx(), initial, callback);
        output
    }

    pub fn filter_map<U: Send + Clone + 'static>(
        &self,
        callback: impl Send + 'static + Fn(T) -> Option<U>,
    ) -> Pipe<U> {
        let output = Pipe::new();
        generic::filter_map(self.subscribe(), output.get_tx(), callback);
        output
    }

    pub fn filter(&self, callback: impl Send + 'static + Fn(&T) -> bool) -> Pipe<T> {
        let output = Pipe::new();
        generic::filter(self.subscribe(), output.get_tx(), callback);
        output
    }

    pub fn gate(&self, gate: Pipe<bool>) -> Pipe<T> {
        let output = Pipe::new();
        generic::gate(self.subscribe(), gate.subscribe(), output.get_tx());
        output
    }

    pub fn startup_delay(&self, duration: Duration, value: T) -> Pipe<T> {
        let output = Pipe::new();
        timers::startup_delay(self.subscribe(), output.get_tx(), duration, value);
        output
    }

    pub fn copy_to(&self, output: Pipe<T>) {
        generic::copy(self.subscribe(), output.get_tx());
    }
}

impl Pipe<bool> {
    pub fn delay_true(&self, duration: Duration) -> Pipe<bool> {
        let output = Pipe::new();
        timers::delay_true(self.subscribe(), output.get_tx(), duration);
        output
    }
    pub fn delay_cancel(&self, duration: Duration) -> Pipe<bool> {
        let output = Pipe::new();
        timers::delay_cancel(self.subscribe(), output.get_tx(), duration);
        output
    }

    pub fn timer_true(&self, duration: Duration) -> Pipe<bool> {
        let output = Pipe::new();
        timers::timer_true(self.subscribe(), output.get_tx(), duration);
        output
    }
}

pub fn requires_plugin(
    battery_level: Pipe<usize>,
    plugged_in: Pipe<bool>,
    geofence: Pipe<String>,
    reminder: Pipe<bool>,
) -> Pipe<bool> {
    let output = Pipe::new();
    teslamate::requires_plugin(
        battery_level.subscribe(),
        plugged_in.subscribe(),
        geofence.subscribe(),
        reminder.subscribe(),
        output.get_tx(),
    );
    output
}

pub fn is_insecure(is_user_present: Pipe<bool>, locked: Pipe<bool>) -> Pipe<bool> {
    let output = Pipe::new();
    teslamate::is_insecure(
        is_user_present.subscribe(),
        locked.subscribe(),
        output.get_tx(),
    );
    output
}
