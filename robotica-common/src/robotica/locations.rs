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

#[derive(Debug, Clone, PartialEq)]
/// A list of locations
pub struct LocationList(Vec<Location>);

impl LocationList {
    /// Create a new location list
    #[must_use]
    pub fn new(list: Vec<Location>) -> Self {
        Self(list)
    }

    /// Is the location at home?
    #[must_use]
    pub fn is_at_home(&self) -> bool {
        self.0.iter().any(Location::is_at_home)
    }

    /// Is the location near home?
    #[must_use]
    pub fn is_near_home(&self) -> bool {
        self.0.iter().any(Location::is_near_home)
    }

    /// Turn the list into a map
    #[must_use]
    pub fn into_map(&self) -> std::collections::HashMap<i32, &Location> {
        self.0.iter().map(|l| (l.id, l)).collect()
    }

    /// Turn the list into a set of ids
    #[must_use]
    pub fn into_set(&self) -> std::collections::HashSet<i32> {
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

/// A location message for an object
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct LocationMessage {
    /// The location of the object
    pub position: geo::Point<f64>,

    /// The locations that the object is in
    pub locations: Vec<Location>,
}
