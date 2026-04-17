//! Track location with Espresence
use std::{collections::HashMap, str::Utf8Error};

use crate::{
    pipes::{stateful, stateless, Subscriber, Subscription},
    spawn,
};
use chrono::{DateTime, Utc};
use robotica_common::{datetime::utc_now, mqtt::MqttMessage, robotica::entities::Id};
use robotica_macro::time_delta_constant;
use serde::Deserialize;
use thiserror::Error;
use tokio::{
    select,
    time::{sleep_until, Instant},
};
use tracing::{debug, warn};

pub use robotica_common::robotica::occupancy::PresenceTrackerValue;

/// The configuration of a Presence Tracker
#[derive(Deserialize, Debug)]
pub struct Config {
    /// A unique identifier for this Presence Tracker.
    pub id: String,
}

#[derive(Deserialize)]
struct EspresenceMessage {
    distance: f32,
}

/// The error type for JSON conversion
#[derive(Error, Debug)]
pub enum EspresenceMessageError {
    /// The payload was not a valid JSON string.
    #[error("Invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),

    /// The payload was not a valid UTF-8 string.
    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] Utf8Error),
}

/// A message from Espresence
#[derive(Clone, Debug)]
pub struct EspresenceMessageWithRoom {
    room: String,
    distance: f32,
}

struct State {
    room: String,
    distance: f32,
    updated: DateTime<Utc>,
    away_instant: Instant,
}

impl TryFrom<MqttMessage> for EspresenceMessageWithRoom {
    type Error = EspresenceMessageError;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        let room = msg
            .topic
            .split('/')
            .next_back()
            .unwrap_or("unknown")
            .to_string();
        let payload: &str = msg.payload_as_str()?;
        let message: EspresenceMessage = serde_json::from_str(payload)?;
        Ok(EspresenceMessageWithRoom {
            room,
            distance: message.distance,
        })
    }
}

/// Run the Presence Tracker code
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn run(
    config: Config,
    espresence_rx: stateless::Receiver<EspresenceMessageWithRoom>,
) -> stateful::Receiver<PresenceTrackerValue> {
    let (tx, rx) = stateful::create_pipe(format!("PresenceTracker_{}", config.id));
    let timeout = time_delta_constant!(0:0:30);
    let away_timeout = time_delta_constant!(0:0:30);

    spawn(async move {
        let mut sub = espresence_rx.subscribe().await;
        let mut maybe_state: Option<State> = None;

        loop {
            select! {
                Some(()) = async {
                    match &maybe_state {
                        Some(state) => {
                            sleep_until(state.away_instant).await;
                            Some(())
                        },
                        None => None,
                    }
                } => {
                    debug!("{}: Away timeout reached", config.id);
                    tx.try_send(PresenceTrackerValue { room: None, distance: None });
                    maybe_state = None;
                }

                Ok(data) = sub.recv() => {
                    debug!("{}: Received data: {:?}", config.id, data);

                    let now = utc_now();
                    let EspresenceMessageWithRoom {room, distance} = data;

                    let new_state = match maybe_state {
                        None => {
                            debug!("{}: First presence detected in room {}", config.id, room);
                            State {
                                room,
                                distance,
                                updated: now,
                                away_instant: calculate_away_instant(away_timeout, now)
                            }
                        },
                        Some(state) => {
                            let duration = now - state.updated;
                            if room == state.room || distance < state.distance || duration > timeout {
                                debug!("{}: Presence updated in room {}/{}/{} duration={} distance={} state.distance={}", config.id, room == state.room, distance < state.distance, duration > timeout, duration, distance, state.distance);
                                State {
                                    room,
                                    distance,
                                    updated: now,
                                    away_instant: calculate_away_instant(away_timeout, now)
                                }
                            } else {
                                debug!("{}: Presence not updated", config.id);
                                state
                            }
                        }
                    };
                    tx.try_send(PresenceTrackerValue { room: Some(new_state.room.clone()), distance: Some(new_state.distance) });
                    maybe_state = Some(new_state);
                }
            }
        }
    });

    rx
}

fn calculate_away_instant(away_timeout: chrono::TimeDelta, updated: DateTime<Utc>) -> Instant {
    let delta = updated + away_timeout - utc_now();
    let std = delta.to_std().unwrap_or(std::time::Duration::from_secs(60));
    Instant::now() + std
}

/// Is there any presence in the given room?
#[must_use]
pub fn is_any_presence_in_room<S: 'static + ::std::hash::BuildHasher + Send>(
    room: &str,
    presences: HashMap<String, stateful::Receiver<PresenceTrackerValue>, S>,
) -> stateful::Receiver<bool> {
    if presences.is_empty() {
        return stateful::static_pipe(false, format!("IsAnyPresenceInRoom_{room}"));
    }

    let (tx, rx) = stateful::create_pipe(format!("IsAnyPresenceInRoom_{room}"));
    let room = room.to_string();

    spawn(async move {
        let mut results = vec![false; presences.len()];
        let receivers = presences.into_values().collect::<Vec<_>>();
        let combined = stateful::combine_latest("combined_presence", receivers);
        let mut combined_sub = combined.subscribe().await;

        while let Ok((i, msg)) = combined_sub.recv().await {
            if let Some(slot) = results.get_mut(i) {
                *slot = msg.room.as_ref() == Some(&room);
            } else {
                tracing::error!(
                    "is_any_presence_in_room: received out-of-bounds index {i} (results.len = {})",
                    results.len()
                );
            }
            tx.try_send(results.iter().any(|r| *r));
        }
    });

    rx
}

/// Get the room for a given presence tracker ID
#[must_use]
pub fn get_room_for_id<S: 'static + ::std::hash::BuildHasher + Send>(
    id: &Id,
    presences: &HashMap<String, stateful::Receiver<PresenceTrackerValue>, S>,
) -> stateful::Receiver<Option<String>> {
    let id = id.to_string();
    let tracker = presences.get(&id).cloned();

    tracker.map_or_else(
        || {
            warn!("No presence tracker found for ID: {}", id);
            stateful::static_entity(None, format!("GetRoomForId_{id}"))
        },
        |tracker| {
            let (tx, rx) = stateful::create_pipe(format!("GetRoomForId_{id}"));
            spawn(async move {
                let mut sub = tracker.subscribe().await;
                while let Ok(msg) = sub.recv().await {
                    tx.try_send(msg.room.clone());
                }
            });
            rx
        },
    )
}
