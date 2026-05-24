//! Thin wrapper around the generic location monitor for `TeslaMate` sources.

use robotica_common::{mqtt::Json, teslamate};
use robotica_tokio::pipes::stateful;

use crate::{
    car,
    monitor_location::{self, AudienceConfig},
};

pub use monitor_location::Outputs;

/// Monitor a `TeslaMate` location stream for a car.
pub fn monitor(
    car: &car::Config,
    location: stateful::Receiver<Json<teslamate::Location>>,
    postgres: sqlx::PgPool,
) -> Outputs {
    monitor_location::monitor(
        "Tesla",
        &car.name,
        AudienceConfig {
            locations: car.audience.locations.clone(),
            private: car.audience.private.clone(),
        },
        location,
        postgres,
    )
}
