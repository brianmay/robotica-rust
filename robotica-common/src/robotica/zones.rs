//! Common types for robotica zone boundaries and location messages.

/// A zone is a named geographic area with a polygonal boundary stored in the database.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct Zone {
    /// The unique id of the zone.
    pub id: i32,

    /// The name of the zone.
    pub name: String,

    /// The boundary of the zone.
    pub bounds: geo::Polygon<f64>,

    /// The color of the polygon
    pub color: String,

    /// Should we announce when something enters this zone?
    pub announce_on_enter: bool,

    /// Should we announce when something exits this zone?
    pub announce_on_exit: bool,
}

impl Zone {
    /// Is this the home zone?
    #[must_use]
    pub fn is_at_home(&self) -> bool {
        self.name == "Home"
    }

    /// Is this the near-home zone?
    #[must_use]
    pub fn is_near_home(&self) -> bool {
        self.name == "Near Home"
    }
}

#[derive(Debug, Clone, PartialEq)]
/// A list of occupied zones derived from a [`LocationMessage`].
pub struct OccupiedZones(Vec<OccupiedZone>);

impl OccupiedZones {
    /// Create a new occupied zones list.
    #[must_use]
    pub const fn new(list: Vec<OccupiedZone>) -> Self {
        Self(list)
    }

    /// Is any zone the home zone?
    #[must_use]
    pub fn is_at_home(&self) -> bool {
        self.0.iter().any(OccupiedZone::is_at_home)
    }

    /// Is any zone the near-home zone?
    #[must_use]
    pub fn is_near_home(&self) -> bool {
        self.0.iter().any(OccupiedZone::is_near_home)
    }

    /// Turn the list into a set of ids.
    #[must_use]
    pub fn to_set(&self) -> std::collections::HashSet<i32> {
        self.0.iter().map(|l| l.id).collect()
    }
}

/// A request to create a new zone.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct CreateZone {
    /// The name of the zone.
    pub name: String,

    /// The boundary of the zone.
    pub bounds: geo::Polygon<f64>,

    /// The color of the polygon
    pub color: String,

    /// Should we announce when something enters this zone?
    pub announce_on_enter: bool,

    /// Should we announce when something exits this zone?
    pub announce_on_exit: bool,
}

/// A lightweight summary of a zone currently occupied by a tracked object.
///
/// Contains only the fields needed by consumers of [`LocationMessage`];
/// the full [`Zone`] (with bounds, color, announce flags, etc.) lives
/// solely in the database and the backend's internal state.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct OccupiedZone {
    /// The unique id of the zone (matches [`Zone::id`]).
    pub id: i32,

    /// The human-readable name of the zone (e.g. `"Home"`, `"Near Home"`).
    pub name: String,

    /// Distance from the tracker to the nearest zone boundary, in metres.
    ///
    /// `0.0` means the tracker is inside or on the boundary of the zone;
    /// positive values mean the tracker is outside (`PostGIS` `ST_Distance`
    /// returns 0 for any point inside a polygon).
    pub distance_m: f64,
}

/// A zone that is within the candidate search radius but not currently occupied.
///
/// Useful for tuning arrival/exit radii: shows how close the tracker came
/// without triggering a zone transition.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct NearbyZone {
    /// The unique id of the zone (matches [`Zone::id`]).
    pub id: i32,

    /// The human-readable name of the zone.
    pub name: String,

    /// Distance from the tracker to the nearest zone boundary, in metres (always ≥ 0).
    pub distance_m: f64,
}

impl OccupiedZone {
    /// Create from a [`Zone`] with a known distance.
    #[must_use]
    pub fn from_zone(zone: &Zone, distance_m: f64) -> Self {
        Self {
            id: zone.id,
            name: zone.name.clone(),
            distance_m,
        }
    }

    /// Is this the home zone?
    #[must_use]
    pub fn is_at_home(&self) -> bool {
        self.name == "Home"
    }

    /// Is this the near-home zone?
    #[must_use]
    pub fn is_near_home(&self) -> bool {
        self.name == "Near Home"
    }
}

impl IntoIterator for OccupiedZones {
    type Item = OccupiedZone;
    type IntoIter = std::vec::IntoIter<OccupiedZone>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// A location message for a tracked object.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct LocationMessage {
    /// Human-readable label identifying the tracked object (e.g. `"Model 3"`, `"Brian's phone"`).
    pub label: String,

    /// The latitude of the object
    pub latitude: f64,

    /// The longitude of the object
    pub longitude: f64,

    /// Timestamp of the location fix.
    #[cfg(feature = "chrono")]
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// The zones the object is currently inside.
    pub locations: Vec<OccupiedZone>,

    /// Zones within the candidate search radius that were not triggered.
    ///
    /// Useful for tuning arrival/exit radii.
    pub nearby_zones: Vec<NearbyZone>,
}
