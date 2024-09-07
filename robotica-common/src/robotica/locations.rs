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
    pub const fn new(list: Vec<Location>) -> Self {
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
    pub fn into_map(self) -> std::collections::HashMap<i32, Location> {
        self.0.into_iter().map(|l| (l.id, l)).collect()
    }

    /// Turn the list into a set of ids
    #[must_use]
    pub fn to_set(&self) -> std::collections::HashSet<i32> {
        self.0.iter().map(|l| l.id).collect()
    }

    /// Turn the list into a sorted list
    // #[must_use]
    // pub fn to_vec(&self) -> Vec<Location> {
    //     let mut list = self.0.clone();
    //     list.sort_by_key(|k| k.id);
    //     list
    // }

    /// Filter out items from list
    pub fn retain(&mut self, f: impl Fn(&Location) -> bool) {
        self.0.retain(f);
    }

    /// Extend the list with another list
    pub fn extend(&mut self, other: impl IntoIterator<Item = Location>) {
        self.0.extend(other);
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
    /// The latitude of the object
    pub latitude: f64,

    /// The longitude of the object
    pub longitude: f64,

    /// The locations that the object is in
    pub locations: Vec<Location>,
}
