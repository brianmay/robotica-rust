use robotica_backend::{
    pipes::{stateful, Subscriber, Subscription},
    spawn,
};
use robotica_common::{
    mqtt::{Json, QoS, Retain},
    robotica::{self, audio::MessagePriority, locations::LocationList},
    teslamate,
};
use tracing::error;

use crate::InitState;

use super::{
    private::{new_message, new_private_message},
    Config,
};

mod state {
    use std::collections::{HashMap, HashSet};

    use robotica_backend::database;
    use robotica_common::{
        robotica::locations::{self, LocationList},
        teslamate,
    };
    use tap::Pipe;

    pub struct State {
        // is_home: bool,
        // is_near_home: bool,
        set: HashSet<i32>,
        map: HashMap<i32, locations::Location>,
    }

    impl State {
        pub fn new(list: locations::LocationList) -> Self {
            let set = list.to_set();
            let map = list.into_map();
            // let is_home = list.is_at_home();
            // let is_near_home = list.is_near_home();
            Self {
                // is_home,
                // is_near_home,
                set,
                map,
            }
        }

        pub async fn search_locations(
            postgres: &sqlx::PgPool,
            location: &teslamate::Location,
            distance: f64,
        ) -> Result<Self, sqlx::Error> {
            let point = geo::Point::new(location.longitude, location.latitude);
            database::locations::search_locations(postgres, point, distance)
                .await?
                .pipe(LocationList::new)
                .pipe(Self::new)
                .pipe(Ok)
        }

        // pub const fn is_at_home(&self) -> bool {
        //     self.is_home
        // }

        // pub const fn is_near_home(&self) -> bool {
        //     self.is_near_home
        // }

        pub fn get(&self, id: i32) -> Option<&locations::Location> {
            self.map.get(&id)
        }

        // pub fn into_set(self) -> HashSet<i32> {
        //     self.set
        // }

        // pub fn into_map(self) -> HashMap<i32, &'a locations::Location> {
        //     self.map
        // }

        pub fn difference(&self, other: &Self) -> HashSet<i32> {
            self.set.difference(&other.set).copied().collect()
        }

        // pub fn iter(&self) -> impl Iterator<Item = &locations::Location> {
        //     self.map.values().copied()
        // }

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

pub struct Outputs {
    // pub lat_lng: stateful::Receiver<robotica::locations::LocationMessage>,
    pub location: stateful::Receiver<LocationList>,
    pub is_home: stateful::Receiver<bool>,
}

pub fn monitor(
    state: &InitState,
    location: stateful::Receiver<Json<teslamate::Location>>,
    postgres: sqlx::PgPool,
    tesla: &Config,
) -> Outputs {
    let (tx, rx) = stateful::create_pipe("teslamate_location");
    let id = tesla.teslamate_id.to_string();
    let mqtt = state.mqtt.clone();
    let message_sink = state.message_sink.clone();

    spawn(async move {
        let mut inputs = location.subscribe().await;
        let mut locations = state::State::new(LocationList::new(vec![]));
        let mut first_time = true;

        while let Ok(Json(location)) = inputs.recv().await {
            let inner_locations = state::State::search_locations(&postgres, &location, 0.0).await;
            let inner_locations = match inner_locations {
                Ok(locations) => locations,
                Err(err) => {
                    error!("Failed to search locations: {}", err);
                    continue;
                }
            };

            let outer_locations = state::State::search_locations(&postgres, &location, 10.0).await;
            let outer_locations = match outer_locations {
                Ok(locations) => locations,
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
                for location in &arrived {
                    let msg = format!("The Tesla arrived at {}", location.name);
                    let msg = if location.announce_on_enter {
                        new_message(msg, MessagePriority::Low)
                    } else {
                        new_private_message(msg, MessagePriority::Low)
                    };
                    message_sink.try_send(msg);
                }

                for location in left {
                    let msg = format!("The Tesla left {}", location.name);
                    let msg = if location.announce_on_exit {
                        new_message(msg, MessagePriority::Low)
                    } else {
                        new_private_message(msg, MessagePriority::Low)
                    };
                    message_sink.try_send(msg);
                }
            }

            locations.reject(&left_set);
            locations.extend(arrived);
            first_time = false;

            let output = robotica::locations::LocationMessage {
                latitude: location.latitude,
                longitude: location.longitude,
                locations: locations.to_vec(),
            };
            mqtt.try_serialize_send(
                format!("state/Tesla/{id}/Locations"),
                &Json(output.clone()),
                Retain::Retain,
                QoS::AtLeastOnce,
            );

            tx.try_send(output);
        }
    });

    let location = rx.map(|(_, l)| LocationList::new(l.locations));
    let is_home = location.clone().map(|(_, l)| l.is_at_home());

    Outputs {
        // lat_lng: rx,
        location,
        is_home,
    }
}
