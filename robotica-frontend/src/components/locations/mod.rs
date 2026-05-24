use robotica_common::robotica::zones::{CreateZone, Zone};

pub mod editor;
pub mod list;
pub mod locations_view;
pub mod map;

#[derive(Debug, Clone, PartialEq)]
pub enum ActionLocation {
    Create(CreateZone),
    Update(Zone),
}

impl ActionLocation {
    fn set_name(&mut self, name: String) {
        match self {
            ActionLocation::Create(zone) => zone.name = name,
            ActionLocation::Update(zone) => zone.name = name,
        }
    }

    fn set_bounds(&mut self, polygon: geo::Polygon) {
        match self {
            ActionLocation::Create(zone) => zone.bounds = polygon,
            ActionLocation::Update(zone) => zone.bounds = polygon,
        }
    }

    fn set_color(&mut self, color: String) {
        match self {
            ActionLocation::Create(zone) => zone.color = color,
            ActionLocation::Update(zone) => zone.color = color,
        }
    }

    const fn set_announce_on_enter(&mut self, announce_on_enter: bool) {
        match self {
            ActionLocation::Create(zone) => zone.announce_on_enter = announce_on_enter,
            ActionLocation::Update(zone) => zone.announce_on_enter = announce_on_enter,
        }
    }

    const fn set_announce_on_exit(&mut self, announce_on_exit: bool) {
        match self {
            ActionLocation::Create(zone) => zone.announce_on_exit = announce_on_exit,
            ActionLocation::Update(zone) => zone.announce_on_exit = announce_on_exit,
        }
    }

    fn name(&self) -> String {
        match self {
            ActionLocation::Create(zone) => zone.name.clone(),
            ActionLocation::Update(zone) => zone.name.clone(),
        }
    }

    fn bounds(&self) -> geo::Polygon {
        match self {
            ActionLocation::Create(zone) => zone.bounds.clone(),
            ActionLocation::Update(zone) => zone.bounds.clone(),
        }
    }

    fn color(&self) -> String {
        match self {
            ActionLocation::Create(zone) => zone.color.clone(),
            ActionLocation::Update(zone) => zone.color.clone(),
        }
    }

    const fn announce_on_enter(&self) -> bool {
        match self {
            ActionLocation::Create(zone) => zone.announce_on_enter,
            ActionLocation::Update(zone) => zone.announce_on_enter,
        }
    }

    const fn announce_on_exit(&self) -> bool {
        match self {
            ActionLocation::Create(zone) => zone.announce_on_exit,
            ActionLocation::Update(zone) => zone.announce_on_exit,
        }
    }
}
