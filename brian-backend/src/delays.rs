use std::time::Duration;

// use tracing::debug;
use robotica_backend::{
    pipes::{stateful, Subscriber},
    spawn,
};
use tokio::{
    select,
    time::{self, sleep_until, Instant, Interval},
};
use tracing::debug;

pub enum DelayInputState<T> {
    Idle,
    Delaying(Instant, T),
    NoDelay,
}

async fn maybe_sleep_until<T>(state: &DelayInputState<T>) -> Option<()>
where
    T: Sync,
{
    if let DelayInputState::Delaying(instant, _) = state {
        sleep_until(*instant).await;
        Some(())
    } else {
        None
    }
}

// pub trait IsActive {
//     fn is_active(&self) -> bool;
// }

#[derive(Default)]
pub struct DelayInputOptions {
    pub skip_subsequent_delay: bool,
}

#[allow(clippy::needless_pass_by_value)]
pub fn delay_input<T>(
    name: &str,
    duration: Duration,
    rx: stateful::Receiver<T>,
    is_active: impl Fn(&stateful::OldNewType<T>) -> bool + Send + 'static,
    options: DelayInputOptions,
) -> stateful::Receiver<T>
where
    T: Clone + Eq + Send + Sync + 'static,
{
    let (tx_out, rx_out) = stateful::create_pipe(name);
    spawn(async move {
        let mut state = DelayInputState::Idle;
        let mut s = rx.subscribe().await;

        loop {
            select! {
                v = s.recv_old_new() => {
                    let Ok(v) = v else { break};

                    // debug!("delay received: {:?}", v);
                    let active_value = is_active(&v);
                    let (_, v) = v;
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
                    state = if options.skip_subsequent_delay { DelayInputState::NoDelay } else { DelayInputState::Idle };
                },
            }
        }
    });
    rx_out
}

pub enum DelayRepeatState<T> {
    Idle,
    Delaying(Interval, T),
}

async fn maybe_tick<T>(state: &mut DelayRepeatState<T>) -> Option<()>
where
    T: Send,
{
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
    rx: stateful::Receiver<T>,
    is_active: impl Fn(&stateful::OldNewType<T>) -> bool + Send + 'static,
) -> stateful::Receiver<T>
where
    T: Clone + Eq + Send + 'static,
{
    let (tx_out, rx_out) = stateful::create_pipe(name);
    spawn(async move {
        let mut state = DelayRepeatState::Idle;
        let mut s = rx.subscribe().await;

        loop {
            select! {
                v = s.recv_old_new() => {
                    let Ok(v) = v else { break};

                    // debug!("delay received: {:?}", v);
                    let active_value = is_active(&v);
                    let (_, v)= v;

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
            }
        }
    });
    rx_out
}

#[derive(Debug)]
pub enum RateLimitState<T> {
    Idle,
    Waiting(Instant),
    Delaying(Instant, T),
}

impl<T: Sync> RateLimitState<T> {
    async fn maybe_sleep_until(&self) -> Option<()> {
        match self {
            Self::Idle => None,
            Self::Waiting(instant) | Self::Delaying(instant, _) => {
                sleep_until(*instant).await;
                Some(())
            }
        }
    }
}

pub fn rate_limit<T>(
    name: &str,
    duration: Duration,
    rx: stateful::Receiver<T>,
) -> stateful::Receiver<T>
where
    T: std::fmt::Debug + Clone + Eq + Send + Sync + 'static,
{
    let (tx_out, rx_out) = stateful::create_pipe(name);
    spawn(async move {
        let mut state = RateLimitState::Idle;
        let mut s = rx.subscribe().await;

        loop {
            select! {
                v = s.recv_old_new() => {
                    let Ok((old, v)) = v else { break};

                    debug!("rate_limit received: {:?}", v);
                    state =
                    {
                        #[allow(clippy::match_same_arms)]
                        match (old.is_some(), state) {
                            (false, _) => {
                                // Don't rate limit initial value
                                tx_out.try_send(v);
                                RateLimitState::Idle
                            },
                            (true, RateLimitState::Idle) => {
                                tx_out.try_send(v);
                                RateLimitState::Waiting(Instant::now() + duration)
                            },
                            (true, RateLimitState::Waiting(instant)) => {
                                RateLimitState::Delaying(instant, v)
                            },
                            (true, RateLimitState::Delaying(instant, _)) => {
                                RateLimitState::Delaying(instant, v)
                            },
                        }
                    };
                },
                Some(()) = state.maybe_sleep_until() => {
                    debug!("rate_limit timer: {:?}", state);
                    state = {
                        #[allow(clippy::match_same_arms)]
                        match state {
                        RateLimitState::Idle => {
                            RateLimitState::Idle
                        },
                        RateLimitState::Waiting(_) => {
                            RateLimitState::Idle
                        },
                        RateLimitState::Delaying(_, v) => {
                            tx_out.try_send(v);
                            RateLimitState::Waiting(Instant::now() + duration)
                        },
                    }
                }
                }
            }
        }
    });
    rx_out
}
