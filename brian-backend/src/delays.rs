use std::fmt::Debug;
use std::time::Duration;

// use log::debug;
use robotica_backend::{entities::create_stateless_entity, spawn};
use tokio::{
    select,
    time::{self, sleep_until, Instant, Interval},
};

pub enum DelayInputState<T> {
    Idle,
    Delaying(Instant, T),
    NoDelay,
}

async fn maybe_sleep_until<T>(state: &DelayInputState<T>) -> Option<()> {
    if let DelayInputState::Delaying(instant, _) = state {
        sleep_until(*instant).await;
        Some(())
    } else {
        None
    }
}

pub trait IsActive {
    fn is_active(&self) -> bool;
}

pub fn delay_input<T>(
    name: &str,
    duration: Duration,
    rx: robotica_backend::entities::Receiver<T>,
) -> robotica_backend::entities::Receiver<T>
where
    T: Clone + Debug + Send + Sync + Eq + IsActive + 'static,
{
    let (tx_out, rx_out) = create_stateless_entity(name);
    spawn(async move {
        let mut state = DelayInputState::Idle;
        let mut s = rx.subscribe().await;

        loop {
            select! {
                Ok(v) = s.recv() => {
                    // debug!("delay received: {:?}", v);
                    let active_value = v.is_active();
                    match (active_value, &state) {
                        (false, _) => {
                            state = DelayInputState::Idle;
                            tx_out.try_send(v);
                        },
                        (true, DelayInputState::Idle) => {
                            state = DelayInputState::Delaying(Instant::now() + duration, v);
                        },
                        (true, DelayInputState::Delaying(instant, _)) => {
                            state = DelayInputState::Delaying(*instant, v);
                        },
                        (true, DelayInputState::NoDelay) => {
                            tx_out.try_send(v);
                        },
                    }

                },
                Some(()) = maybe_sleep_until(&state) => {
                    if let DelayInputState::Delaying(_, v) = state {
                        // debug!("delay timer, sending: {:?}", v);
                        tx_out.try_send(v);
                    } else {
                        // debug!("delay timer, not sending anything (shouldn't happen)");
                    }
                    state = DelayInputState::NoDelay;
                },
                else => { break; }
            }
        }
    });
    rx_out
}

pub enum DelayRepeatState<T> {
    Idle,
    Delaying(Interval, T),
}

async fn maybe_tick<T>(state: &mut DelayRepeatState<T>) -> Option<()> {
    if let DelayRepeatState::Delaying(interval, _) = state {
        interval.tick().await;
        Some(())
    } else {
        None
    }
}

pub fn delay_repeat<T>(
    name: &str,
    duration: Duration,
    rx: robotica_backend::entities::Receiver<T>,
) -> robotica_backend::entities::Receiver<T>
where
    T: Clone + Debug + Send + Sync + Eq + IsActive + 'static,
{
    let (tx_out, rx_out) = create_stateless_entity(name);
    spawn(async move {
        let mut state = DelayRepeatState::Idle;
        let mut s = rx.subscribe().await;

        loop {
            select! {
                Ok(v) = s.recv() => {
                    // debug!("delay received: {:?}", v);
                    let active_value = v.is_active();
                    match (active_value, state) {
                        (false, _) => {
                            state = DelayRepeatState::Idle;
                            tx_out.try_send(v);
                        },
                        (true, DelayRepeatState::Idle) => {
                            state = DelayRepeatState::Delaying(time::interval(duration), v);
                        },
                        (true, DelayRepeatState::Delaying(i, _)) => {
                            state = DelayRepeatState::Delaying(i, v);
                        },
                    }
                },
                Some(()) = maybe_tick(&mut state) => {
                    if let DelayRepeatState::Delaying(_, v) = &state {
                        tx_out.try_send(v.clone());
                    } else {
                        // debug!("delay timer, not sending anything (shouldn't happen)");
                    }
                },
                else => { break; }
            }
        }
    });
    rx_out
}
