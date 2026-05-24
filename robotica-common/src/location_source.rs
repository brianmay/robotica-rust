//! Trait for types that can supply a geographic location with a timestamp.

use chrono::{DateTime, Utc};

use crate::{owntracks, teslamate};

/// A type that can provide a latitude, longitude, and timestamp.
///
/// Implement this trait to use a type as the input to the generic location
/// monitor pipeline.
pub trait LocationSource {
    /// Latitude in degrees.
    fn latitude(&self) -> f64;

    /// Longitude in degrees.
    fn longitude(&self) -> f64;

    /// Timestamp of the location fix.
    ///
    /// For sources that do not carry a timestamp (e.g. `teslamate::Location`)
    /// this should return the current wall-clock time.
    fn timestamp(&self) -> DateTime<Utc>;
}

impl LocationSource for teslamate::Location {
    fn latitude(&self) -> f64 {
        self.latitude
    }

    fn longitude(&self) -> f64 {
        self.longitude
    }

    /// `teslamate::Location` carries no timestamp, so the current time is returned.
    fn timestamp(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

impl LocationSource for owntracks::LocationMessage {
    fn latitude(&self) -> f64 {
        self.lat
    }

    fn longitude(&self) -> f64 {
        self.lon
    }

    fn timestamp(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.tst, 0).unwrap_or_else(Utc::now)
    }
}
