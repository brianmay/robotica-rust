use std::fmt::Debug;
use std::time::Duration;

// use log::debug;
use robotica_node_rust::{
    entities::{create_stateful_entity, StatefulData},
    spawn,
};
use tokio::{
    select,
    time::{sleep_until, Instant},
};

pub enum DelayState<T> {
    Idle,
    Delaying(Instant, T),
    NoDelay,
}

async fn maybe_sleep_until<T>(state: &DelayState<T>) -> Option<()> {
    if let DelayState::Delaying(instant, _) = state {
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
    rx: robotica_node_rust::entities::Receiver<T>,
) -> robotica_node_rust::entities::Receiver<StatefulData<T>>
where
    T: Clone + Debug + Send + Sync + Eq + IsActive + 'static,
{
    let (tx_out, rx_out) = create_stateful_entity(name);
    spawn(async move {
        let mut state = DelayState::Idle;
        let mut s = rx.subscribe().await;

        loop {
            select! {
                Ok(v) = s.recv() => {
                    // debug!("delay received: {:?}", v);
                    let active_value = v.is_active();
                    match (active_value, &state) {
                        (false, _) => {
                            state = DelayState::Idle;
                            tx_out.send(v).await;
                        },
                        (true, DelayState::Idle) => {
                            state = DelayState::Delaying(Instant::now() + duration, v);
                        },
                        (true, DelayState::Delaying(instant, _)) => {
                            state = DelayState::Delaying(*instant, v);
                        },
                        (true, DelayState::NoDelay) => {
                            tx_out.send(v).await;
                        },
                    }

                },
                Some(()) = maybe_sleep_until(&state) => {
                    if let DelayState::Delaying(_, v) = state {
                        // debug!("delay timer, sending: {:?}", v);
                        tx_out.send(v).await;
                    } else {
                        // debug!("delay timer, not sending anything (shouldn't happen)");
                    }
                    state = DelayState::NoDelay;
                },
                else => { break; }
            }
        }
    });
    rx_out
}
