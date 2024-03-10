use robotica_common::robotica::locations::{CreateLocation, Location};

pub mod control;
pub mod item_map;
pub mod list_map;
pub mod locations_view;

#[derive(Debug, Clone, PartialEq)]
pub enum ActionLocation {
    Create(CreateLocation),
    Update(Location),
}

impl ActionLocation {
    fn set_name(&mut self, name: String) {
        match self {
            ActionLocation::Create(location) => location.name = name,
            ActionLocation::Update(location) => location.name = name,
        }
    }

    fn set_bounds(&mut self, polygon: geo::Polygon) {
        match self {
            ActionLocation::Create(location) => location.bounds = polygon,
            ActionLocation::Update(location) => location.bounds = polygon,
        }
    }

    fn name(&self) -> String {
        match self {
            ActionLocation::Create(location) => location.name.clone(),
            ActionLocation::Update(location) => location.name.clone(),
        }
    }

    fn bounds(&self) -> geo::Polygon {
        match self {
            ActionLocation::Create(location) => location.bounds.clone(),
            ActionLocation::Update(location) => location.bounds.clone(),
        }
    }
}
