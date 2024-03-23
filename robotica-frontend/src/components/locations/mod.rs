use robotica_common::robotica::locations::{CreateLocation, Location};

pub mod editor;
pub mod list;
pub mod locations_view;
pub mod map;

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

    fn set_color(&mut self, color: String) {
        match self {
            ActionLocation::Create(location) => location.color = color,
            ActionLocation::Update(location) => location.color = color,
        }
    }

    fn set_announce_on_enter(&mut self, announce_on_enter: bool) {
        match self {
            ActionLocation::Create(location) => location.announce_on_enter = announce_on_enter,
            ActionLocation::Update(location) => location.announce_on_enter = announce_on_enter,
        }
    }

    fn set_announce_on_exit(&mut self, announce_on_exit: bool) {
        match self {
            ActionLocation::Create(location) => location.announce_on_exit = announce_on_exit,
            ActionLocation::Update(location) => location.announce_on_exit = announce_on_exit,
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

    fn color(&self) -> String {
        match self {
            ActionLocation::Create(location) => location.color.clone(),
            ActionLocation::Update(location) => location.color.clone(),
        }
    }

    const fn announce_on_enter(&self) -> bool {
        match self {
            ActionLocation::Create(location) => location.announce_on_enter,
            ActionLocation::Update(location) => location.announce_on_enter,
        }
    }

    const fn announce_on_exit(&self) -> bool {
        match self {
            ActionLocation::Create(location) => location.announce_on_exit,
            ActionLocation::Update(location) => location.announce_on_exit,
        }
    }
}
