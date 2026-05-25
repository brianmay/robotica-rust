//! Generic location monitoring pipeline.
//!
//! Takes a stream of any type implementing [`LocationSource`] and produces
//! enriched location outputs by cross-referencing against the database.

use robotica_common::location_source::LocationSource;
use robotica_common::{
    mqtt::Json,
    robotica::{
        self,
        audio::MessagePriority,
        message::{Audience, Message},
        zones::{NearbyZone, OccupiedZones},
    },
};
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

    use robotica_common::robotica::zones::{self, OccupiedZone};
    use robotica_tokio::database;

    pub struct State {
        set: HashSet<i32>,
        /// Maps zone id → (Zone, signed distance in metres).
        map: HashMap<i32, (zones::Zone, f64)>,
    }

    impl State {
        pub fn empty() -> Self {
            Self {
                set: HashSet::new(),
                map: HashMap::new(),
            }
        }

        /// Search for zones within `distance` metres and build a State from them.
        ///
        /// Distances are signed: negative = inside, positive = outside.
        pub async fn search_locations(
            postgres: &sqlx::PgPool,
            lat: f64,
            lon: f64,
            distance: f64,
        ) -> Result<Self, sqlx::Error> {
            let point = geo::Point::new(lon, lat);
            let zones = database::zones::search_zones(postgres, point, distance).await?;
            let set = zones.iter().map(|l| l.id).collect();
            // We don't have individual distances here; use 0.0 as a sentinel
            // (this path is only used for arrival/exit hysteresis, not for reporting).
            let map = zones.into_iter().map(|l| (l.id, (l, 0.0_f64))).collect();
            Ok(Self { set, map })
        }

        /// Search for all zones within `candidate_radius` metres and return them
        /// with signed distances.
        pub async fn search_with_distance(
            postgres: &sqlx::PgPool,
            lat: f64,
            lon: f64,
            candidate_radius: f64,
        ) -> Result<Vec<(zones::Zone, f64)>, sqlx::Error> {
            let point = geo::Point::new(lon, lat);
            database::zones::search_zones_with_distance(postgres, point, candidate_radius).await
        }

        pub fn get(&self, id: i32) -> Option<&zones::Zone> {
            self.map.get(&id).map(|(l, _)| l)
        }

        pub fn difference(&self, other: &Self) -> HashSet<i32> {
            self.set.difference(&other.set).copied().collect()
        }

        pub fn to_vec(&self) -> Vec<OccupiedZone> {
            let mut list = self
                .map
                .values()
                .map(|(loc, dist)| OccupiedZone::from_zone(loc, *dist))
                .collect::<Vec<_>>();
            list.sort_by_key(|k| k.id);
            list
        }

        pub fn extend(&mut self, zones: Vec<(zones::Zone, f64)>) {
            for (zone, dist) in zones {
                self.set.insert(zone.id);
                self.map.insert(zone.id, (zone, dist));
            }
        }

        pub fn reject(&mut self, hs: &HashSet<i32>) {
            self.set.retain(|x| !hs.contains(x));
            self.map.retain(|k, _v| !hs.contains(k));
        }

        /// Refresh distances for all currently-occupied zones from the latest
        /// candidate query results.
        pub fn update_distances(&mut self, candidates: &[(zones::Zone, f64)]) {
            for (loc, dist) in candidates {
                if let Some(entry) = self.map.get_mut(&loc.id) {
                    entry.1 = *dist;
                }
            }
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
    pub location: stateful::Receiver<OccupiedZones>,
    /// `true` when the object is at home.
    pub is_home: stateful::Receiver<bool>,
    /// Arrival / departure messages.
    pub messages: stateless::Receiver<Message>,
    /// Full location message (lat/lon + location list).
    pub location_message: stateful::Receiver<robotica::zones::LocationMessage>,
}

fn new_message(
    title: &str,
    message: impl Into<String>,
    priority: MessagePriority,
    audience: impl Into<Audience>,
) -> Message {
    Message::new(title, message.into(), priority, audience)
}

/// Candidate radius used when querying nearby zones for observability.
///
/// All zones within this distance (metres) are fetched; those within
/// `arrival_radius` are treated as occupied, the rest are reported as
/// [`NearbyZone`]s.
const CANDIDATE_RADIUS_M: f64 = 500.0;

#[allow(clippy::too_many_arguments)]
async fn process_location(
    lat: f64,
    lon: f64,
    timestamp: chrono::DateTime<chrono::Utc>,
    postgres: &sqlx::PgPool,
    locations: &mut state::State,
    first_time: &mut bool,
    title: &str,
    tracked_name: &str,
    audience: &AudienceConfig,
    arrival_radius: f64,
    exit_radius: f64,
    message_tx: &stateless::Sender<Message>,
) -> Option<robotica::zones::LocationMessage> {
    let inner_locations =
        match state::State::search_locations(postgres, lat, lon, arrival_radius).await {
            Ok(l) => l,
            Err(err) => {
                error!("Failed to search locations: {}", err);
                return None;
            }
        };
    let outer_locations =
        match state::State::search_locations(postgres, lat, lon, exit_radius).await {
            Ok(l) => l,
            Err(err) => {
                error!("Failed to search locations: {}", err);
                return None;
            }
        };

    // --- candidate query for distances + nearby_zones ---
    let candidates =
        match state::State::search_with_distance(postgres, lat, lon, CANDIDATE_RADIUS_M).await {
            Ok(c) => c,
            Err(err) => {
                error!("Failed to search candidate locations: {}", err);
                return None;
            }
        };

    let arrived: Vec<_> = inner_locations
        .difference(locations)
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

    if !*first_time {
        for loc in &arrived {
            let msg = format!("{tracked_name} arrived at {}", loc.name);
            let aud = if loc.announce_on_enter {
                &audience.locations
            } else {
                &audience.private
            };
            message_tx.try_send(new_message(title, msg, MessagePriority::Low, aud.clone()));
        }
        for loc in left {
            let msg = format!("{tracked_name} left {}", loc.name);
            let aud = if loc.announce_on_exit {
                &audience.locations
            } else {
                &audience.private
            };
            message_tx.try_send(new_message(title, msg, MessagePriority::Low, aud.clone()));
        }
    }

    let arrived_with_dist: Vec<_> = arrived
        .into_iter()
        .map(|loc| {
            let dist = candidates
                .iter()
                .find(|(c, _)| c.id == loc.id)
                .map_or(0.0, |(_, d)| *d);
            (loc, dist)
        })
        .collect();

    locations.reject(&left_set);
    locations.extend(arrived_with_dist);
    locations.update_distances(&candidates);
    *first_time = false;

    let occupied_ids: std::collections::HashSet<i32> =
        locations.to_vec().iter().map(|z| z.id).collect();
    let mut nearby_zones: Vec<NearbyZone> = candidates
        .iter()
        .filter(|(loc, _)| !occupied_ids.contains(&loc.id))
        .map(|(loc, dist)| NearbyZone {
            id: loc.id,
            name: loc.name.clone(),
            distance_m: *dist,
        })
        .collect();
    nearby_zones.sort_by_key(|z| z.id);

    Some(robotica::zones::LocationMessage {
        label: tracked_name.to_owned(),
        latitude: lat,
        longitude: lon,
        timestamp,
        zones: locations.to_vec(),
        nearby_zones,
    })
}

/// Monitor a stream of location updates, enriching each with database lookups.
///
/// * `title` — title used in arrival/departure messages
///   (e.g. `"Tesla"` or `"OwnTracks"`).
/// * `tracked_name` — human-readable name for the tracked object used in
///   message bodies (e.g. `"Model 3"` or `"Brian's phone"`).
/// * `audience` — where to send arrival/departure announcements.
/// * `location` — upstream pipe of location updates.
/// * `postgres` — database pool for location lookups.
/// * `arrival_radius` — extra padding in metres added to zone boundaries when
///   testing whether the object has arrived (use `0.0` for exact boundary).
/// * `exit_radius` — extra padding in metres used for the exit hysteresis test;
///   the object must move this far outside the zone before departure is triggered.
pub fn monitor<T>(
    title: impl Into<String>,
    tracked_name: impl Into<String>,
    audience: AudienceConfig,
    location: stateful::Receiver<Json<T>>,
    postgres: sqlx::PgPool,
    arrival_radius: f64,
    exit_radius: f64,
) -> Outputs
where
    T: LocationSource + Send + Sync + Clone + 'static,
{
    let (location_tx, location_rx) = stateful::create_pipe("location_monitor");
    let (message_tx, message_rx) = stateless::create_pipe("location_monitor_message");

    let title = title.into();
    let tracked_name = tracked_name.into();

    spawn(async move {
        let mut inputs = location.subscribe().await;
        let mut locations = state::State::empty();
        let mut first_time = true;

        while let Ok(Json(location)) = inputs.recv().await {
            if let Some(output) = process_location(
                location.latitude(),
                location.longitude(),
                location.timestamp(),
                &postgres,
                &mut locations,
                &mut first_time,
                &title,
                &tracked_name,
                &audience,
                arrival_radius,
                exit_radius,
                &message_tx,
            )
            .await
            {
                location_tx.try_send(output);
            }
        }
    });

    let location = location_rx
        .clone()
        .map(|(_, l)| OccupiedZones::new(l.zones));
    let is_home = location.clone().map(|(_, l)| l.is_at_home());

    Outputs {
        location,
        is_home,
        messages: message_rx,
        location_message: location_rx,
    }
}
