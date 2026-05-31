use robotica_common::robotica::zones::{CreateZone, Zone};

pub mod editor;
pub mod list;
pub mod map;
pub mod zones;

#[derive(Debug, Clone, PartialEq)]
pub enum ActionZone {
    Create(CreateZone),
    Update(Zone),
}

impl ActionZone {
    #[must_use]
    pub const fn id(&self) -> Option<i32> {
        match self {
            ActionZone::Create(_) => None,
            ActionZone::Update(zone) => Some(zone.id),
        }
    }

    fn set_name(&mut self, name: String) {
        match self {
            ActionZone::Create(zone) => zone.name = name,
            ActionZone::Update(zone) => zone.name = name,
        }
    }

    fn set_bounds(&mut self, polygon: geo::Polygon) {
        match self {
            ActionZone::Create(zone) => zone.bounds = polygon,
            ActionZone::Update(zone) => zone.bounds = polygon,
        }
    }

    fn set_color(&mut self, color: String) {
        match self {
            ActionZone::Create(zone) => zone.color = color,
            ActionZone::Update(zone) => zone.color = color,
        }
    }

    const fn set_announce_on_enter(&mut self, announce_on_enter: bool) {
        match self {
            ActionZone::Create(zone) => zone.announce_on_enter = announce_on_enter,
            ActionZone::Update(zone) => zone.announce_on_enter = announce_on_enter,
        }
    }

    const fn set_announce_on_exit(&mut self, announce_on_exit: bool) {
        match self {
            ActionZone::Create(zone) => zone.announce_on_exit = announce_on_exit,
            ActionZone::Update(zone) => zone.announce_on_exit = announce_on_exit,
        }
    }

    fn name(&self) -> String {
        match self {
            ActionZone::Create(zone) => zone.name.clone(),
            ActionZone::Update(zone) => zone.name.clone(),
        }
    }

    fn bounds(&self) -> geo::Polygon {
        match self {
            ActionZone::Create(zone) => zone.bounds.clone(),
            ActionZone::Update(zone) => zone.bounds.clone(),
        }
    }

    fn color(&self) -> String {
        match self {
            ActionZone::Create(zone) => zone.color.clone(),
            ActionZone::Update(zone) => zone.color.clone(),
        }
    }

    const fn announce_on_enter(&self) -> bool {
        match self {
            ActionZone::Create(zone) => zone.announce_on_enter,
            ActionZone::Update(zone) => zone.announce_on_enter,
        }
    }

    const fn announce_on_exit(&self) -> bool {
        match self {
            ActionZone::Create(zone) => zone.announce_on_exit,
            ActionZone::Update(zone) => zone.announce_on_exit,
        }
    }
}
