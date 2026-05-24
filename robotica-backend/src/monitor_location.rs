//! Generic location monitoring pipeline.
//!
//! Takes a stream of any type implementing [`LocationSource`] and produces
//! enriched location outputs by cross-referencing against the database.

use robotica_common::{
    mqtt::Json,
    robotica::{
        self,
        audio::MessagePriority,
        locations::LocationList,
        message::{Audience, Message},
    },
};
use robotica_common::location_source::LocationSource;
use robotica_tokio::{
    pipes::{stateful, stateless, Subscriber, Subscription},
    spawn,
};
use tracing::error;

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

mod state {
    use std::collections::{HashMap, HashSet};

    use robotica_common::robotica::locations::{self, LocationList};
    use robotica_tokio::database;
    use tap::Pipe;

    pub struct State {
        set: HashSet<i32>,
        map: HashMap<i32, locations::Location>,
    }

    impl State {
        pub fn new(list: LocationList) -> Self {
            let set = list.to_set();
            let map = list.into_map();
            Self { set, map }
        }

        pub async fn search_locations(
            postgres: &sqlx::PgPool,
            lat: f64,
            lon: f64,
            distance: f64,
        ) -> Result<Self, sqlx::Error> {
            let point = geo::Point::new(lon, lat);
            database::locations::search_locations(postgres, point, distance)
                .await?
                .pipe(LocationList::new)
                .pipe(Self::new)
                .pipe(Ok)
        }

        pub fn get(&self, id: i32) -> Option<&locations::Location> {
            self.map.get(&id)
        }

        pub fn difference(&self, other: &Self) -> HashSet<i32> {
            self.set.difference(&other.set).copied().collect()
        }

        pub fn to_vec(&self) -> Vec<locations::Location> {
            let mut list = self.map.values().cloned().collect::<Vec<_>>();
            list.sort_by_key(|k| k.id);
            list
        }

        pub fn extend(&mut self, locations: Vec<locations::Location>) {
            for location in locations {
                self.set.insert(location.id);
                self.map.insert(location.id, location);
            }
        }

        pub fn reject(&mut self, hs: &HashSet<i32>) {
            self.set.retain(|x| !hs.contains(x));
            self.map.retain(|k, _v| !hs.contains(k));
        }
    }
}

// ---------------------------------------------------------------------------
// Public interface
// ---------------------------------------------------------------------------

/// Audience configuration for location announcements.
pub struct AudienceConfig {
    /// Audience for location-related announcements (e.g. "arrived at X").
    pub locations: Audience,
    /// Audience for private announcements.
    pub private: Audience,
}

/// Outputs produced by [`monitor`].
pub struct Outputs {
    /// The current set of named locations the tracked object is inside.
    pub location: stateful::Receiver<LocationList>,
    /// `true` when the object is at home.
    pub is_home: stateful::Receiver<bool>,
    /// Arrival / departure messages.
    pub messages: stateless::Receiver<Message>,
    /// Full location message (lat/lon + location list).
    pub location_message: stateful::Receiver<robotica::locations::LocationMessage>,
}

fn new_message(
    sender_name: &str,
    message: impl Into<String>,
    priority: MessagePriority,
    audience: impl Into<Audience>,
) -> Message {
    Message::new(sender_name, message.into(), priority, audience)
}

/// Monitor a stream of location updates, enriching each with database lookups.
///
/// * `sender_name` — name used as the sender in arrival/departure messages
///   (e.g. `"Tesla"` or `"Phone"`).
/// * `tracked_name` — human-readable name for the tracked object used in
///   message bodies (e.g. `"Model 3"` or `"Brian's phone"`).
/// * `audience` — where to send arrival/departure announcements.
/// * `location` — upstream pipe of location updates.
/// * `postgres` — database pool for location lookups.
pub fn monitor<T>(
    sender_name: impl Into<String>,
    tracked_name: impl Into<String>,
    audience: AudienceConfig,
    location: stateful::Receiver<Json<T>>,
    postgres: sqlx::PgPool,
) -> Outputs
where
    T: LocationSource + Send + Sync + Clone + 'static,
{
    let (location_tx, location_rx) = stateful::create_pipe("location_monitor");
    let (message_tx, message_rx) = stateless::create_pipe("location_monitor_message");

    let sender_name = sender_name.into();
    let tracked_name = tracked_name.into();

    spawn(async move {
        let mut inputs = location.subscribe().await;
        let mut locations = state::State::new(LocationList::new(vec![]));
        let mut first_time = true;

        while let Ok(Json(location)) = inputs.recv().await {
            let lat = location.latitude();
            let lon = location.longitude();

            let inner_locations =
                state::State::search_locations(&postgres, lat, lon, 0.0).await;
            let inner_locations = match inner_locations {
                Ok(l) => l,
                Err(err) => {
                    error!("Failed to search locations: {}", err);
                    continue;
                }
            };

            let outer_locations =
                state::State::search_locations(&postgres, lat, lon, 10.0).await;
            let outer_locations = match outer_locations {
                Ok(l) => l,
                Err(err) => {
                    error!("Failed to search locations: {}", err);
                    continue;
                }
            };

            let arrived: Vec<_> = inner_locations
                .difference(&locations)
                .into_iter()
                .filter_map(|id| inner_locations.get(id))
                .cloned()
                .collect();

            let left_set = locations.difference(&outer_locations);

            let left: Vec<_> = left_set
                .iter()
                .copied()
                .filter_map(|id| locations.get(id))
                .collect();

            if !first_time {
                for loc in &arrived {
                    let msg = format!("{tracked_name} arrived at {}", loc.name);
                    let aud = if loc.announce_on_enter {
                        &audience.locations
                    } else {
                        &audience.private
                    };
                    let msg = new_message(&sender_name, msg, MessagePriority::Low, aud.clone());
                    message_tx.try_send(msg);
                }

                for loc in left {
                    let msg = format!("{tracked_name} left {}", loc.name);
                    let aud = if loc.announce_on_exit {
                        &audience.locations
                    } else {
                        &audience.private
                    };
                    let msg = new_message(&sender_name, msg, MessagePriority::Low, aud.clone());
                    message_tx.try_send(msg);
                }
            }

            locations.reject(&left_set);
            locations.extend(arrived);
            first_time = false;

            let output = robotica::locations::LocationMessage {
                latitude: location.latitude(),
                longitude: location.longitude(),
                locations: locations.to_vec(),
            };
            location_tx.try_send(output);
        }
    });

    let location = location_rx
        .clone()
        .map(|(_, l)| LocationList::new(l.locations));
    let is_home = location.clone().map(|(_, l)| l.is_at_home());

    Outputs {
        location,
        is_home,
        messages: message_rx,
        location_message: location_rx,
    }
}
