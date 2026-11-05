//! Shared front end configuration
//!
//! The UI configuration is a list of views. Every screen the UI can show is
//! a [`ViewConfig`], which carries its identity (`id`), display properties
//! (`title`, `menu`), access control (`groups`) and its kind.
//!
//! Each view kind derives the MQTT topics it subscribes to and sends on
//! (see [`ViewConfig::topics`]). Topic derivation is exhaustive: adding a
//! new view kind or controller variant without declaring its topics is a
//! compile error, so the configuration and the topic permissions derived
//! from it can never fall out of sync.

use serde::{Deserialize, Serialize};

use crate::{
    controllers::{
        robotica::{hdmi_matrix, lights, music, switch},
        Action, ConfigTrait, {tasmota, zwave},
    },
    mqtt::topics::Topics,
    robotica::entities::Id,
};

/// Configuration for a button Controller
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ControllerConfig {
    /// This is a HDMI Matrix button
    HdmiMatrix(hdmi_matrix::Config),

    /// This is a light button
    Light(lights::Config),

    /// This is a music button
    Music(music::Config),

    /// This is a switch button
    Switch(switch::Config),

    /// This is a zwave button
    Zwave(zwave::Config),

    /// This is a tasmota button
    Tasmota(tasmota::Config),
}

impl ControllerConfig {
    /// The MQTT topics this controller subscribes to and sends on.
    ///
    /// This is an exhaustive match: a new controller variant cannot be added
    /// without also declaring its topics.
    #[must_use]
    pub fn topics(&self) -> Topics {
        match self {
            ControllerConfig::HdmiMatrix(config) => config.topics(),
            ControllerConfig::Light(config) => config.topics(),
            ControllerConfig::Music(config) => config.topics(),
            ControllerConfig::Switch(config) => config.topics(),
            ControllerConfig::Zwave(config) => config.topics(),
            ControllerConfig::Tasmota(config) => config.topics(),
        }
    }
}

/// An Icon for a button
#[derive(Deserialize, Serialize, Copy, Clone, Eq, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Icon {
    /// This is a fan icon
    Fan,
    /// This is a light icon
    Light,
    /// This is a night icon
    Night,
    /// This is a schedule icon
    Schedule,
    /// This is a selection icon
    Select,
    /// This is a speaker icon
    Speaker,
    /// This is a trumpet icon
    Trumpet,
    /// This is a tv icon
    Tv,
}

/// Configuration for a button
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct ButtonConfig {
    /// The id for this button
    pub id: String,
    /// The controller for this button
    pub controller: ControllerConfig,
    /// The title for this button
    pub title: String,
    /// The icon for this button
    pub icon: Icon,
}

/// Configuration for a button row
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct ButtonRowConfig {
    /// The id for this button row
    pub id: String,
    /// The title for this button row
    pub title: String,
    /// The buttons for this button row
    pub buttons: Vec<ButtonConfig>,
}

/// Configuration for a controls view: a grid of button rows.
///
/// Formerly known as a "room"; renamed because a room is a physical
/// location, while this is a view that happens to control devices in one.
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct ControlsConfig {
    /// The button rows for this view
    pub rows: Vec<ButtonRowConfig>,
}

/// Configuration for a car view.
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct CarConfig {
    /// The groups allowed to use the charging controls (the auto charge
    /// toggle, and the minimum charge and end time settings). Empty
    /// means everyone who can see the view.
    #[serde(default)]
    pub control_groups: Vec<String>,

    /// Are the charging controls enabled for the recipient of this
    /// configuration?
    ///
    /// Set during per user filtering based on `control_groups`; true in
    /// the source configuration by default. Can also be set to false in
    /// the configuration to disable the controls for everyone.
    #[serde(default = "default_controls_enabled")]
    pub controls: bool,
}

/// The default for [`CarConfig::controls`].
#[must_use]
const fn default_controls_enabled() -> bool {
    true
}

/// Configuration for a locations view.
///
/// The view tracks the location of a declared set of objects. Only the
/// declared objects are visible, which allows restricting which objects a
/// user can track by giving them access to a view with a restricted object
/// list.
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct LocationsConfig {
    /// The ids of the objects (people, devices) to track.
    pub objects: Vec<String>,
}

/// Configuration for an occupancy view.
///
/// The view shows which declared people are present, plus the occupied
/// state of all rooms.
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct OccupancyConfig {
    /// The ids of the people to track.
    pub people: Vec<String>,
}

/// The kind of a view, and its kind specific configuration.
///
/// The variant list is exhaustive: every screen the UI can show must be
/// represented here, and every variant must declare its topics in
/// [`ViewConfig::topics`].
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    /// A grid of control buttons
    Controls(ControlsConfig),

    /// A car charging view; the view id is the car entity id
    Car(CarConfig),

    /// A water heater view; the view id is the water heater entity id
    WaterHeater,

    /// A schedule view
    Schedule,

    /// A tags view
    Tags,

    /// A prices view
    Prices,

    /// A locations view
    Locations(LocationsConfig),

    /// An occupancy view
    Occupancy(OccupancyConfig),
}

/// Configuration for a view
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct ViewConfig {
    /// The id for this view.
    ///
    /// This is the stable key used by routes and by devices referencing
    /// views. For entity backed kinds ( [`ViewKind::Car`],
    /// [`ViewKind::WaterHeater`] ) it is also the entity id.
    pub id: String,

    /// The title for this view, for display only.
    ///
    /// Never used as a key; views are always referenced by
    /// [`ViewConfig::id`].
    pub title: String,

    /// The name of the menu this view appears in.
    ///
    /// When set, the view appears in a dropdown menu of this name;
    /// otherwise it appears as a top-level navigation item.
    pub menu: Option<String>,

    /// The groups that are allowed to access this view.
    ///
    /// When empty, all authenticated users are allowed; otherwise the user
    /// must be a member of at least one of the listed groups.
    #[serde(default)]
    pub groups: Vec<String>,

    /// The kind of this view, and its kind specific configuration.
    pub kind: ViewKind,
}

impl ViewConfig {
    /// Is this view allowed for a user with the given group names?
    ///
    /// A view with no groups is allowed for everyone; otherwise the user
    /// must be a member of at least one of the view's groups.
    #[must_use]
    pub fn is_allowed(&self, user_groups: &[String]) -> bool {
        self.groups.is_empty() || self.groups.iter().any(|group| user_groups.contains(group))
    }

    /// The view as served to a user with the given groups.
    ///
    /// The view itself must be allowed (see [`ViewConfig::is_allowed`]);
    /// this filters the per view options to the user's permissions, for
    /// example stripping the car charging controls.
    #[must_use]
    pub fn filtered_for(&self, user_groups: &[String]) -> ViewConfig {
        let mut view = self.clone();
        if let ViewKind::Car(car) = &mut view.kind {
            car.controls = car.controls
                && (car.control_groups.is_empty()
                    || car
                        .control_groups
                        .iter()
                        .any(|group| user_groups.contains(group)));
        }
        view
    }

    /// The MQTT topics this view subscribes to and sends on.
    ///
    /// This is an exhaustive match: a new view kind cannot be added without
    /// also declaring its topics.
    #[must_use]
    pub fn topics(&self, instance: &str) -> Topics {
        let mut topics = Topics::default();

        match &self.kind {
            ViewKind::Controls(controls) => {
                for row in &controls.rows {
                    for button in &row.buttons {
                        topics.merge(&button.controller.topics());
                    }
                }
            }
            ViewKind::Car(car) => {
                topics.subscribe_exact(amber_entity_topic(&self.id));

                // The charging controls in the car view: the auto charge
                // switch button, and the charge schedule commands (which
                // mirror `AnyId::get_command_topic`). Only present when
                // the controls are enabled for the recipient.
                if car.controls {
                    topics.merge(&car_auto_charge_switch(&self.id).topics());
                    topics.send_exact(format!("robotica/command/{}/min_charge_tomorrow", self.id));
                    topics.send_exact(format!("robotica/command/{}/set_charge_end_time", self.id));
                }
            }
            ViewKind::WaterHeater => {
                topics.subscribe_exact(amber_entity_topic(&self.id));
            }
            ViewKind::Schedule => {
                topics.subscribe_exact(schedule_topic(instance));
            }
            ViewKind::Tags => {
                topics.subscribe_exact(tags_topic(instance));
            }
            ViewKind::Prices => {
                topics.subscribe_exact(amber_topic("prices"));
                topics.subscribe_exact(amber_topic("tick"));
            }
            ViewKind::Locations(locations) => {
                for object in &locations.objects {
                    topics.subscribe_exact(location_topic(object));
                }
            }
            ViewKind::Occupancy(occupancy) => {
                for person in &occupancy.people {
                    topics.subscribe_exact(presence_topic(person));
                }
                topics.subscribe_pattern(occupancy_topic_pattern().to_string());
            }
        }

        topics
    }
}

/// The pseudo-entity id used for the amber account state topics.
pub const AMBER_ACCOUNT_ID: &str = "amber_account";

/// Get an amber account state topic (e.g. prices or tick).
#[must_use]
pub fn amber_topic(name: &str) -> String {
    format!("robotica/state/{AMBER_ACCOUNT_ID}/{name}")
}

/// Get the amber state topic for an entity (a car or water heater).
#[must_use]
pub fn amber_entity_topic(id: &str) -> String {
    format!("robotica/state/{id}/amber")
}

/// The switch configuration for a car's auto charge button.
///
/// Used by the car view (to derive its topics) and by the car view
/// component (for the button itself), so the two cannot drift apart.
#[must_use]
pub fn car_auto_charge_switch(id: &str) -> switch::Config {
    switch::Config {
        topic_substr: format!("{id}/auto_charge"),
        action: Action::Toggle,
    }
}

/// Get the schedule topic for an instance.
#[must_use]
pub fn schedule_topic(instance: &str) -> String {
    format!("schedule/{instance}/pending")
}

/// Get the tags topic for an instance.
#[must_use]
pub fn tags_topic(instance: &str) -> String {
    format!("robotica/{instance}/tags")
}

/// Get the location topic for an object.
#[must_use]
pub fn location_topic(object: &str) -> String {
    format!("robotica/state/{object}/locations")
}

/// Get the presence topic for a person.
#[must_use]
pub fn presence_topic(person: &str) -> String {
    format!("robotica/state/{person}/presence")
}

/// Get the wildcard pattern that covers all occupancy sensors.
#[must_use]
pub const fn occupancy_topic_pattern() -> &'static str {
    "robotica/state/+/+/occupancy"
}

/// Configuration for the UI
#[derive(Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct Config {
    /// The name of the server in MQTT topics
    pub instance: String,

    /// The views for this UI
    pub views: Vec<ViewConfig>,
}

impl Config {
    /// Get a view by id.
    #[must_use]
    pub fn get_view(&self, id: &str) -> Option<&ViewConfig> {
        self.views.iter().find(|view| view.id == id)
    }

    /// The config filtered to the views a user with the given group names
    /// is allowed to access, with per view options filtered to their
    /// permissions.
    ///
    /// This is what a user is served: they only see the views (and
    /// therefore the navigation items) they have access to, and within a
    /// view only the options they are permitted to use.
    #[must_use]
    pub fn filter_allowed(&self, user_groups: &[String]) -> Config {
        Config {
            instance: self.instance.clone(),
            views: self
                .views
                .iter()
                .filter(|view| view.is_allowed(user_groups))
                .map(|view| view.filtered_for(user_groups))
                .collect(),
        }
    }

    /// The MQTT topics required by all views.
    ///
    /// This is the union of the topics of every view, and is the basis of
    /// the topic permissions derived for a user with access to all views.
    #[must_use]
    pub fn topics(&self) -> Topics {
        let mut topics = Topics::default();
        for view in &self.views {
            topics.merge(&view.topics(&self.instance));
        }
        topics
    }
}

/// Convert a view id into an entity id, if valid.
///
/// # Errors
///
/// Returns [`crate::robotica::entities::Error`] if the id is not a valid
/// entity id.
pub fn view_id_to_entity(id: &str) -> Result<Id, crate::robotica::entities::Error> {
    Id::new(id)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::controllers::Action;

    fn button(controller: ControllerConfig) -> ButtonConfig {
        ButtonConfig {
            id: "button".to_string(),
            controller,
            title: "Button".to_string(),
            icon: Icon::Light,
        }
    }

    fn controls_view(controller: ControllerConfig) -> ViewConfig {
        ViewConfig {
            id: "kitchen".to_string(),
            title: "Kitchen".to_string(),
            menu: Some("Downstairs".to_string()),
            groups: Vec::new(),
            kind: ViewKind::Controls(ControlsConfig {
                rows: vec![ButtonRowConfig {
                    id: "row".to_string(),
                    title: "Row".to_string(),
                    buttons: vec![button(controller)],
                }],
            }),
        }
    }

    fn view(id: &str, kind: ViewKind) -> ViewConfig {
        ViewConfig {
            id: id.to_string(),
            title: id.to_string(),
            menu: None,
            groups: Vec::new(),
            kind,
        }
    }

    #[test]
    fn test_controls_topics_tasmota() {
        let controller = ControllerConfig::Tasmota(tasmota::Config {
            topic_substr: "fan".to_string(),
            action: Action::Toggle,
            power_postfix: String::new(),
        });
        let topics = controls_view(controller).topics("home");
        assert!(topics.is_subscribe_allowed("stat/fan/POWER"));
        assert!(topics.is_subscribe_allowed("tele/fan/LWT"));
        assert!(topics.is_send_allowed("cmnd/fan/POWER"));
    }

    #[test]
    fn test_controls_topics_light() {
        let controller = ControllerConfig::Light(lights::Config {
            topic_substr: "lounge/light".to_string(),
            action: Action::Toggle,
            scene: crate::robotica::lights::SceneName::new("bright"),
        });
        let topics = controls_view(controller).topics("home");
        assert!(topics.is_subscribe_allowed("robotica/state/lounge/light/scene"));
        assert!(topics.is_subscribe_allowed("robotica/state/lounge/light/power"));
        assert!(topics.is_send_allowed("robotica/command/lounge/light"));
    }

    #[test]
    fn test_car_topics() {
        let car = ViewKind::Car(CarConfig {
            control_groups: vec![],
            controls: true,
        });
        let topics = view("tesla_1", car).topics("home");
        assert!(topics.is_subscribe_allowed("robotica/state/tesla_1/amber"));
        assert!(!topics.is_subscribe_allowed("robotica/state/other_car/amber"));

        // The charging controls in the car view.
        assert!(topics.is_subscribe_allowed("robotica/state/tesla_1/auto_charge/power"));
        assert!(topics.is_send_allowed("robotica/command/tesla_1/auto_charge"));
        assert!(topics.is_send_allowed("robotica/command/tesla_1/min_charge_tomorrow"));
        assert!(topics.is_send_allowed("robotica/command/tesla_1/set_charge_end_time"));
        assert!(!topics.is_send_allowed("robotica/command/other_car/auto_charge"));
    }

    #[test]
    fn test_car_controls_restricted_to_group() {
        let car = ViewKind::Car(CarConfig {
            control_groups: vec!["admin".to_string()],
            controls: true,
        });
        let config = Config {
            instance: "home".to_string(),
            views: vec![view("tesla_1", car)],
        };

        // An admin keeps the controls, and their control topics are
        // allowed.
        let admin_config = config.filter_allowed(&["admin".to_string()]);
        assert!(matches!(
            &admin_config.get_view("tesla_1").unwrap().kind,
            ViewKind::Car(car) if car.controls
        ));
        let admin_topics = admin_config.topics();
        assert!(admin_topics.is_send_allowed("robotica/command/tesla_1/auto_charge"));
        assert!(admin_topics.is_send_allowed("robotica/command/tesla_1/set_charge_end_time"));

        // A user without the group still sees the car (the amber state
        // topic is allowed) but the controls are stripped and none of
        // the control topics are allowed.
        let guest_config = config.filter_allowed(&[]);
        assert!(matches!(
            &guest_config.get_view("tesla_1").unwrap().kind,
            ViewKind::Car(car) if !car.controls
        ));
        let guest_topics = guest_config.topics();
        assert!(guest_topics.is_subscribe_allowed("robotica/state/tesla_1/amber"));
        assert!(!guest_topics.is_subscribe_allowed("robotica/state/tesla_1/auto_charge/power"));
        assert!(!guest_topics.is_send_allowed("robotica/command/tesla_1/auto_charge"));
        assert!(!guest_topics.is_send_allowed("robotica/command/tesla_1/min_charge_tomorrow"));
        assert!(!guest_topics.is_send_allowed("robotica/command/tesla_1/set_charge_end_time"));
    }

    #[test]
    fn test_car_controls_default_everyone() {
        // No control groups: everyone keeps the controls.
        let car = ViewKind::Car(CarConfig {
            control_groups: vec![],
            controls: true,
        });
        let config = Config {
            instance: "home".to_string(),
            views: vec![view("tesla_1", car)],
        };

        let guest_config = config.filter_allowed(&[]);
        assert!(matches!(
            &guest_config.get_view("tesla_1").unwrap().kind,
            ViewKind::Car(car) if car.controls
        ));
    }

    #[test]
    fn test_water_heater_topics() {
        let topics = view("hot_water", ViewKind::WaterHeater).topics("home");
        assert!(topics.is_subscribe_allowed("robotica/state/hot_water/amber"));

        // The water heater view is display only; it has no controls.
        assert!(!topics.is_send_allowed("robotica/command/hot_water/auto_charge"));
        assert!(!topics.is_send_allowed("robotica/command/hot_water/min_charge_tomorrow"));
    }

    #[test]
    fn test_schedule_topics() {
        let topics = view("schedule", ViewKind::Schedule).topics("home");
        assert!(topics.is_subscribe_allowed("schedule/home/pending"));
        assert!(!topics.is_subscribe_allowed("schedule/other/pending"));
    }

    #[test]
    fn test_tags_topics() {
        let topics = view("tags", ViewKind::Tags).topics("home");
        assert!(topics.is_subscribe_allowed("robotica/home/tags"));
    }

    #[test]
    fn test_prices_topics() {
        let topics = view("prices", ViewKind::Prices).topics("home");
        assert!(topics.is_subscribe_allowed("robotica/state/amber_account/prices"));
        assert!(topics.is_subscribe_allowed("robotica/state/amber_account/tick"));
    }

    #[test]
    fn test_locations_topics_only_declared_objects() {
        let topics = view(
            "locations",
            ViewKind::Locations(LocationsConfig {
                objects: vec!["brian".to_string(), "ellie".to_string()],
            }),
        )
        .topics("home");

        assert!(topics.is_subscribe_allowed("robotica/state/brian/locations"));
        assert!(topics.is_subscribe_allowed("robotica/state/ellie/locations"));
        assert!(!topics.is_subscribe_allowed("robotica/state/guest/locations"));
    }

    #[test]
    fn test_occupancy_topics_declared_people_and_room_pattern() {
        let topics = view(
            "occupancy",
            ViewKind::Occupancy(OccupancyConfig {
                people: vec!["brian".to_string()],
            }),
        )
        .topics("home");

        assert!(topics.is_subscribe_allowed("robotica/state/brian/presence"));
        assert!(!topics.is_subscribe_allowed("robotica/state/ellie/presence"));
        assert!(topics.is_subscribe_allowed("robotica/state/lounge/default/occupancy"));
        assert!(topics.is_subscribe_allowed("robotica/state/bath/sensor1/occupancy"));
        assert!(!topics.is_subscribe_allowed("robotica/state/lounge/presence"));
    }

    #[test]
    fn test_config_topics_union() {
        let config = Config {
            instance: "home".to_string(),
            views: vec![
                view("schedule", ViewKind::Schedule),
                view("tags", ViewKind::Tags),
            ],
        };

        let topics = config.topics();
        assert!(topics.is_subscribe_allowed("schedule/home/pending"));
        assert!(topics.is_subscribe_allowed("robotica/home/tags"));
        assert!(!topics.is_subscribe_allowed("robotica/home/other"));
    }

    #[test]
    fn test_view_allowed_groups() {
        let mut v = view("v", ViewKind::Schedule);
        assert!(v.is_allowed(&[]), "no groups means everyone is allowed");

        v.groups = vec!["family".to_string()];
        assert!(!v.is_allowed(&[]));
        assert!(v.is_allowed(&["family".to_string()]));
        assert!(v.is_allowed(&["admin".to_string(), "family".to_string()]));
        assert!(!v.is_allowed(&["admin".to_string()]));
    }

    #[test]
    fn test_filter_allowed() {
        let mut kitchen = view("kitchen", ViewKind::Schedule);
        kitchen.groups = vec!["family".to_string()];
        let guest_view = view("guest", ViewKind::Tags);

        let config = Config {
            instance: "home".to_string(),
            views: vec![kitchen, guest_view],
        };

        // A user with no groups sees only the views with no group
        // restriction.
        let guest_config = config.filter_allowed(&[]);
        assert_eq!(guest_config.views.len(), 1);
        assert_eq!(guest_config.views[0].id, "guest");

        // A family member sees both.
        let family_config = config.filter_allowed(&["family".to_string()]);
        assert_eq!(family_config.views.len(), 2);

        // The filtered config's derived topics only include the allowed
        // views' topics.
        let guest_topics = guest_config.topics();
        assert!(guest_topics.is_subscribe_allowed("robotica/home/tags"));
        assert!(!guest_topics.is_subscribe_allowed("schedule/home/pending"));
        let family_topics = family_config.topics();
        assert!(family_topics.is_subscribe_allowed("schedule/home/pending"));
        assert!(family_topics.is_subscribe_allowed("robotica/home/tags"));
    }

    #[test]
    fn test_get_view() {
        let config = Config {
            instance: "home".to_string(),
            views: vec![view(
                "kitchen",
                ViewKind::Controls(ControlsConfig { rows: vec![] }),
            )],
        };
        assert!(config.get_view("kitchen").is_some());
        assert!(config.get_view("lounge").is_none());
    }

    #[test]
    fn test_view_id_to_entity() {
        assert!(view_id_to_entity("kitchen_fan").is_ok());
        assert!(view_id_to_entity("kitchen fan").is_err());
    }

    #[test]
    fn test_deserialize_view() {
        let json = r#"{
            "instance": "home",
            "views": [
                {
                    "id": "kitchen",
                    "title": "Kitchen",
                    "menu": "Downstairs",
                    "groups": ["family"],
                    "kind": { "type": "controls", "rows": [] }
                },
                {
                    "id": "brian_car",
                    "title": "Brian's Car",
                    "menu": "Cars",
                    "kind": { "type": "car" }
                },
                {
                    "id": "prices",
                    "title": "Prices",
                    "kind": { "type": "prices" }
                },
                {
                    "id": "locations",
                    "title": "Locations",
                    "kind": { "type": "locations", "objects": ["brian"] }
                }
            ]
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.instance, "home");
        assert_eq!(config.views.len(), 4);

        let kitchen = config.get_view("kitchen").unwrap();
        assert_eq!(kitchen.menu.as_deref(), Some("Downstairs"));
        assert_eq!(kitchen.groups, vec!["family".to_string()]);
        assert!(matches!(kitchen.kind, ViewKind::Controls(_)));

        let car = config.get_view("brian_car").unwrap();
        assert_eq!(car.menu.as_deref(), Some("Cars"));
        assert!(matches!(car.kind, ViewKind::Car(_)));

        let prices = config.get_view("prices").unwrap();
        assert!(prices.menu.is_none());
        assert!(prices.groups.is_empty(), "groups defaults to empty");
        assert!(matches!(prices.kind, ViewKind::Prices));

        let locations = config.get_view("locations").unwrap();
        assert!(matches!(
            &locations.kind,
            ViewKind::Locations(LocationsConfig { objects }) if objects == &vec!["brian".to_string()]
        ));
    }
}
