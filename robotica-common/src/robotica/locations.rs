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

impl LocationMessage {
    /// Is the object at home?
    #[must_use]
    pub fn is_at_home(&self) -> bool {
        self.locations.iter().any(|l| l.name == "Home")
    }
}
