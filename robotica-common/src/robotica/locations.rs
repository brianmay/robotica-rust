//! Common stuff for robotica locations

/// A location is a named area with a polygonal boundary
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct Location {
    /// The unique id of the location.
    pub id: i32,

    /// The name of the location.
    pub name: String,

    /// The boundary of the location.
    pub bounds: geo::Polygon<f64>,

    /// The color of the polygon
    pub color: String,

    /// Should we announce when something enters this location?
    pub announce_on_enter: bool,

    /// Should we announce when something enters this location?
    pub announce_on_exit: bool,
}

impl Location {
    /// Is the location the home location?
    #[must_use]
    pub fn is_at_home(&self) -> bool {
        self.name == "Home"
    }

    /// Is the location near the home location?
    #[must_use]
    pub fn is_near_home(&self) -> bool {
        self.name == "Near Home"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A list of occupied zones, derived from a [`LocationMessage`].
pub struct LocationList(Vec<OccupiedZone>);

impl LocationList {
    /// Create a new location list
    #[must_use]
    pub const fn new(list: Vec<OccupiedZone>) -> Self {
        Self(list)
    }

    /// Is the location at home?
    #[must_use]
    pub fn is_at_home(&self) -> bool {
        self.0.iter().any(OccupiedZone::is_at_home)
    }

    /// Is the location near home?
    #[must_use]
    pub fn is_near_home(&self) -> bool {
        self.0.iter().any(OccupiedZone::is_near_home)
    }

    /// Turn the list into a set of ids
    #[must_use]
    pub fn to_set(&self) -> std::collections::HashSet<i32> {
        self.0.iter().map(|l| l.id).collect()
    }
}

/// A request to create a new location.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct CreateLocation {
    /// The name of the location.
    pub name: String,

    /// The boundary of the location.
    pub bounds: geo::Polygon<f64>,

    /// The color of the polygon
    pub color: String,

    /// Should we announce when something enters this location?
    pub announce_on_enter: bool,

    /// Should we announce when something enters this location?
    pub announce_on_exit: bool,
}

/// A lightweight summary of a zone currently occupied by a tracked object.
///
/// Contains only the fields needed by consumers of [`LocationMessage`];
/// the full [`Location`] (with bounds, color, announce flags, etc.) lives
/// solely in the database and the backend's internal state.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]
pub struct OccupiedZone {
    /// The unique id of the zone (matches [`Location::id`]).
    pub id: i32,

    /// The human-readable name of the zone (e.g. `"Home"`, `"Near Home"`).
    pub name: String,
}

impl OccupiedZone {
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

impl From<&Location> for OccupiedZone {
    fn from(loc: &Location) -> Self {
        Self {
            id: loc.id,
            name: loc.name.clone(),
        }
    }
}

impl IntoIterator for LocationList {
    type Item = OccupiedZone;
    type IntoIter = std::vec::IntoIter<OccupiedZone>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
/// A location message for an object
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct LocationMessage {
    /// Human-readable label identifying the tracked object (e.g. `"Model 3"`, `"Brian's phone"`).
    pub label: String,

    /// The latitude of the object
    pub latitude: f64,

    /// The longitude of the object
    pub longitude: f64,

    /// The locations that the object is in
    pub locations: Vec<OccupiedZone>,
}
