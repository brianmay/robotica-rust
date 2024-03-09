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
}

/// A request to create a new location.
#[derive(Debug, serde::Deserialize, Clone, PartialEq)]
pub struct CreateLocation {
    /// The name of the location.
    pub name: String,

    /// The boundary of the location.
    pub bounds: geo::Polygon<f64>,
}
