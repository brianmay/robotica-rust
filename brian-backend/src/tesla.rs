use crate::amber::car::ChargeRequest;
use crate::audience;
use crate::delays::{delay_input, delay_repeat, DelayInputOptions};

use anyhow::Result;
use chrono::{DateTime, TimeDelta, Timelike, Utc};
use reqwest::Url;
use robotica_backend::services::persistent_state::{self, PersistentStateRow};
use robotica_backend::services::tesla::api::{
    ChargingStateEnum, CommandSequence, SequenceError, Token, TokenError, VehicleId,
};
use robotica_common::datetime::duration;
use robotica_common::robotica::audio::MessagePriority;
use robotica_common::robotica::commands::Command;
use robotica_common::robotica::locations::LocationList;
use robotica_common::robotica::message::Message;
use robotica_common::robotica::switch::{DeviceAction, DevicePower};
use robotica_common::{robotica, teslamate, unsafe_time_delta};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::ops::Add;
use std::time::Duration;
use tap::Pipe;
use thiserror::Error;
use tokio::select;
use tokio::time::sleep_until;
use tracing::{debug, error, info};

use robotica_backend::pipes::{stateful, stateless, Subscriber, Subscription};
use robotica_backend::spawn;
use robotica_common::mqtt::{BoolError, Json, MqttMessage, Parsed, QoS, Retain};

use super::InitState;

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct TeslamateId(u32);

impl TeslamateId {
    #[cfg(test)]
    pub const fn testing_value() -> Self {
        Self(99)
    }
}

impl ToString for TeslamateId {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

pub struct Receivers {
    pub location: stateful::Receiver<Json<teslamate::Location>>,
    pub charging_state: stateful::Receiver<ChargingStateEnum>,
    pub is_charging: stateful::Receiver<bool>,
    pub battery_level: stateful::Receiver<Parsed<u8>>,
    pub charge_limit: stateful::Receiver<Parsed<u8>>,
    pub frunk: stateful::Receiver<DoorState>,
    pub boot: stateful::Receiver<DoorState>,
    pub doors: stateful::Receiver<DoorState>,
    pub windows: stateful::Receiver<DoorState>,
    pub user_present: stateful::Receiver<UserIsPresent>,
    pub auto_charge: stateless::Receiver<Json<Command>>,
    pub min_charge_tomorrow: stateless::Receiver<Parsed<u8>>,
}

impl Receivers {
    pub fn new(config: &Config, state: &mut InitState) -> Self {
        let id = config.teslamate_id.to_string();

        let location = state
            .subscriptions
            .subscribe_into_stateful::<Json<teslamate::Location>>(&format!(
                "teslamate/cars/{id}/location"
            ));
        let charging_state = state
            .subscriptions
            .subscribe_into_stateful::<ChargingStateEnum>(&format!(
                "teslamate/cars/{id}/charging_state"
            ));
        let battery_level = state
            .subscriptions
            .subscribe_into_stateful::<Parsed<u8>>(&format!("teslamate/cars/{id}/battery_level"));
        let charge_limit = state
            .subscriptions
            .subscribe_into_stateful::<Parsed<u8>>(&format!(
                "teslamate/cars/{id}/charge_limit_soc"
            ));
        let frunk = state
            .subscriptions
            .subscribe_into_stateful::<DoorState>(&format!("teslamate/cars/{id}/frunk_open"));
        let boot = state
            .subscriptions
            .subscribe_into_stateful::<DoorState>(&format!("teslamate/cars/{id}/trunk_open"));
        let doors = state
            .subscriptions
            .subscribe_into_stateful::<DoorState>(&format!("teslamate/cars/{id}/doors_open"));
        let windows = state
            .subscriptions
            .subscribe_into_stateful::<DoorState>(&format!("teslamate/cars/{id}/windows_open"));
        let user_present = state
            .subscriptions
            .subscribe_into_stateful::<UserIsPresent>(&format!(
                "teslamate/cars/{id}/is_user_present"
            ));

        let auto_charge = state
            .subscriptions
            .subscribe_into_stateless::<Json<Command>>(&format!("command/Tesla/{id}/AutoCharge"));
        let min_charge_tomorrow =
            state
                .subscriptions
                .subscribe_into_stateless::<Parsed<u8>>(&format!(
                    "teslamate/cars/{id}/min_charge_tomorrow"
                ));

        let is_charging = charging_state.clone().map(|(_, c)| c.is_charging());

        Self {
            location,
            charging_state,
            is_charging,
            battery_level,
            charge_limit,
            frunk,
            boot,
            doors,
            windows,
            user_present,
            auto_charge,
            min_charge_tomorrow,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(tag = "type")]
pub enum TeslamateAuth {
    #[default]
    None,
    Basic {
        username: String,
        password: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TeslamateConfig {
    pub url: Url,

    #[serde(default)]
    pub auth: TeslamateAuth,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub teslamate_id: TeslamateId,
    pub tesla_id: VehicleId,
    pub teslamate: TeslamateConfig,
    pub name: String,
}

fn new_message(message: impl Into<String>, priority: MessagePriority) -> Message {
    Message::new("Tesla", message.into(), priority, audience::everyone())
}

fn new_private_message(message: impl Into<String>, priority: MessagePriority) -> Message {
    Message::new("Tesla", message.into(), priority, audience::brian(true))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DoorState {
    Open,
    Closed,
}

impl Display for DoorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::Closed => write!(f, "closed"),
        }
    }
}

impl TryFrom<MqttMessage> for DoorState {
    type Error = StateErr;
    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        match msg.try_into() {
            Ok(true) => Ok(Self::Open),
            Ok(false) => Ok(Self::Closed),
            Err(err) => Err(err.into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserIsPresent {
    UserPresent,
    UserNotPresent,
}

impl Display for UserIsPresent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserPresent => write!(f, "user is present"),
            Self::UserNotPresent => write!(f, "user is not present"),
        }
    }
}

impl TryFrom<MqttMessage> for UserIsPresent {
    type Error = StateErr;
    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        match msg.try_into() {
            Ok(true) => Ok(Self::UserPresent),
            Ok(false) => Ok(Self::UserNotPresent),
            Err(err) => Err(err.into()),
        }
    }
}

#[derive(Error, Debug)]
pub enum StateErr {
    #[error("Invalid door state: {0}")]
    InvalidDoorState(#[from] BoolError),

    #[error("Invalid UTF8")]
    Utf8Error(#[from] std::str::Utf8Error),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Door {
    Frunk,
    Boot,
    Doors,
    #[allow(dead_code)]
    Windows,
}

impl Door {
    const fn is_plural(&self) -> bool {
        match self {
            Self::Boot | Self::Frunk => false,
            Self::Doors | Self::Windows => true,
        }
    }
}

impl Display for Door {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Frunk => write!(f, "frunk"),
            Self::Boot => write!(f, "boot"),
            Self::Doors => write!(f, "doors"),
            Self::Windows => write!(f, "windows"),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ShouldPlugin {
    ShouldPlugin,
    NoActionRequired,
}

pub struct Outputs {
    pub lat_lng: stateful::Receiver<robotica::locations::LocationMessage>,
    pub location: stateful::Receiver<LocationList>,
    pub is_home: stateful::Receiver<bool>,
}

pub fn monitor_teslamate_location(
    state: &InitState,
    location: stateful::Receiver<Json<teslamate::Location>>,
    postgres: sqlx::PgPool,
    tesla: &Config,
) -> Outputs {
    let (tx, rx) = stateful::create_pipe("teslamate_location");
    let id = tesla.teslamate_id.to_string();
    let mqtt = state.mqtt.clone();
    let message_sink = state.message_sink.clone();

    spawn(async move {
        let mut inputs = location.subscribe().await;
        let mut locations = state::State::new(LocationList::new(vec![]));
        let mut first_time = true;

        while let Ok(Json(location)) = inputs.recv().await {
            let inner_locations = state::State::search_locations(&postgres, &location, 0.0).await;
            let inner_locations = match inner_locations {
                Ok(locations) => locations,
                Err(err) => {
                    error!("Failed to search locations: {}", err);
                    continue;
                }
            };

            let outer_locations = state::State::search_locations(&postgres, &location, 10.0).await;
            let outer_locations = match outer_locations {
                Ok(locations) => locations,
                Err(err) => {
                    error!("Failed to search locations: {}", err);
                    continue;
                }
            };

            let arrived: Vec<_> = inner_locations
                .difference(&locations)
                .into_iter()
                .filter_map(|id| inner_locations.get(id))
                .cloned()
                .collect();

            let left_set = locations.difference(&outer_locations);

            let left: Vec<_> = left_set
                .iter()
                .copied()
                .filter_map(|id| locations.get(id))
                .collect();

            if !first_time {
                for location in &arrived {
                    let msg = format!("The Tesla arrived at {}", location.name);
                    let msg = if location.announce_on_enter {
                        new_message(msg, MessagePriority::Low)
                    } else {
                        new_private_message(msg, MessagePriority::Low)
                    };
                    message_sink.try_send(msg);
                }

                for location in left {
                    let msg = format!("The Tesla left {}", location.name);
                    let msg = if location.announce_on_exit {
                        new_message(msg, MessagePriority::Low)
                    } else {
                        new_private_message(msg, MessagePriority::Low)
                    };
                    message_sink.try_send(msg);
                }
            }

            locations.reject(&left_set);
            locations.extend(arrived);
            first_time = false;

            let output = robotica::locations::LocationMessage {
                latitude: location.latitude,
                longitude: location.longitude,
                locations: locations.to_vec(),
            };
            mqtt.try_serialize_send(
                format!("state/Tesla/{id}/Locations"),
                &Json(output.clone()),
                Retain::Retain,
                QoS::AtLeastOnce,
            );

            tx.try_send(output);
        }
    });

    let location = rx.clone().map(|(_, l)| LocationList::new(l.locations));
    let is_home = location.clone().map(|(_, l)| l.is_at_home());

    Outputs {
        lat_lng: rx,
        location,
        is_home,
    }
}

mod state {
    use std::collections::{HashMap, HashSet};

    use robotica_backend::database;
    use robotica_common::{
        robotica::locations::{self, LocationList},
        teslamate,
    };
    use tap::Pipe;

    pub struct State {
        // is_home: bool,
        // is_near_home: bool,
        set: HashSet<i32>,
        map: HashMap<i32, locations::Location>,
    }

    impl State {
        pub fn new(list: locations::LocationList) -> Self {
            let set = list.to_set();
            let map = list.into_map();
            // let is_home = list.is_at_home();
            // let is_near_home = list.is_near_home();
            Self {
                // is_home,
                // is_near_home,
                set,
                map,
            }
        }

        pub async fn search_locations(
            postgres: &sqlx::PgPool,
            location: &teslamate::Location,
            distance: f64,
        ) -> Result<Self, sqlx::Error> {
            let point = geo::Point::new(location.longitude, location.latitude);
            database::locations::search_locations(postgres, point, distance)
                .await?
                .pipe(LocationList::new)
                .pipe(Self::new)
                .pipe(Ok)
        }

        // pub const fn is_at_home(&self) -> bool {
        //     self.is_home
        // }

        // pub const fn is_near_home(&self) -> bool {
        //     self.is_near_home
        // }

        pub fn get(&self, id: i32) -> Option<&locations::Location> {
            self.map.get(&id)
        }

        // pub fn into_set(self) -> HashSet<i32> {
        //     self.set
        // }

        // pub fn into_map(self) -> HashMap<i32, &'a locations::Location> {
        //     self.map
        // }

        pub fn difference(&self, other: &Self) -> HashSet<i32> {
            self.set.difference(&other.set).copied().collect()
        }

        // pub fn iter(&self) -> impl Iterator<Item = &locations::Location> {
        //     self.map.values().copied()
        // }

        pub fn to_vec(&self) -> Vec<locations::Location> {
            let mut list = self.map.values().cloned().collect::<Vec<_>>();
            list.sort_by_key(|k| k.id);
            list
        }

        pub fn extend(&mut self, locations: Vec<locations::Location>) {
            for location in locations {
                self.set.insert(location.id);
                self.map.insert(location.id, location);
            }
        }

        pub fn reject(&mut self, hs: &HashSet<i32>) {
            self.set.retain(|x| !hs.contains(x));
            self.map.retain(|k, _v| !hs.contains(k));
        }
    }
}

pub fn monitor_tesla_location(
    tesla: &Config,
    state: &InitState,
    location_stream: stateful::Receiver<LocationList>,
    charging_info: stateful::Receiver<ChargingInformation>,
) -> stateful::Receiver<ShouldPlugin> {
    let message_sink = state.message_sink.clone();
    let (tx, rx) = stateful::create_pipe("tesla_should_plugin");

    let tesla = tesla.clone();

    spawn(async move {
        let mut location_s = location_stream.subscribe().await;
        let mut charging_info_s = charging_info.subscribe().await;
        let name = &tesla.name;

        let Ok(mut old_location) = location_s.recv().await else {
            error!("{name}: Failed to get initial Tesla location");
            return;
        };

        let Ok(mut old_charging_info) = charging_info_s.recv().await else {
            error!("{name}: Failed to get initial Tesla charging information");
            return;
        };

        debug!("{name}: Initial Tesla location: {:?}", old_location);
        debug!(
            "{name}: Initial Tesla charging information: {:?}",
            old_charging_info
        );

        loop {
            let should_plugin = if old_location.is_at_home()
                && !old_charging_info.charging_state.is_plugged_in()
                && old_charging_info.battery_level <= 80
            {
                ShouldPlugin::ShouldPlugin
            } else {
                ShouldPlugin::NoActionRequired
            };
            tx.try_send(should_plugin);

            select! {
                Ok(new_charging_info) = charging_info_s.recv() => {
                    if old_location.is_at_home()  {
                        announce_charging_state(&tesla, &old_charging_info, &new_charging_info, &message_sink);
                    }
                    old_charging_info = new_charging_info;
                },
                Ok(new_location) = location_s.recv() => {
                    if !old_location.is_near_home() && new_location.is_near_home() {
                        let level = old_charging_info.battery_level;

                        let (limit_type, limit) = match old_charging_info.charge_request_at_home {
                            ChargeRequest::ChargeTo(limit) => ("auto", limit),
                            ChargeRequest::Manual => ("manual", old_charging_info.charge_limit),
                        };
                        let msg = if level < limit {
                            format!("{name} is at {level}% and would {limit_type} charge to {limit}%")
                        } else {
                            format!("{name} is at {level}% and the {limit_type} limit is {limit}%")
                        };
                        let msg = new_message(msg, MessagePriority::DaytimeOnly);
                        message_sink.try_send(msg);
                    }

                    old_location = new_location;
                }

            }
        }
    });

    rx
}

pub fn plug_in_reminder(
    state: &InitState,
    tesla: &Config,
    should_plugin_stream: stateful::Receiver<ShouldPlugin>,
) {
    let message_sink = state.message_sink.clone();
    let tesla = tesla.clone();

    let should_plugin_stream = delay_repeat(
        "tesla_should_plugin (repeat)",
        Duration::from_secs(60 * 10),
        should_plugin_stream,
        |(_, should_plugin)| *should_plugin == ShouldPlugin::ShouldPlugin,
    );

    spawn(async move {
        let mut s = should_plugin_stream.subscribe().await;
        while let Ok(should_plugin) = s.recv().await {
            let time = chrono::Local::now();
            if time.hour() >= 18 && time.hour() <= 22 && should_plugin == ShouldPlugin::ShouldPlugin
            {
                let name = &tesla.name;
                let msg = new_message(
                    format!("{name} might run away and should be leashed"),
                    MessagePriority::Low,
                );
                message_sink.try_send(msg);
            }
        }
    });
}

pub struct MonitorDoorsReceivers {
    pub frunk: stateful::Receiver<DoorState>,
    pub boot: stateful::Receiver<DoorState>,
    pub doors: stateful::Receiver<DoorState>,
    pub windows: stateful::Receiver<DoorState>,
    pub user_present: stateful::Receiver<UserIsPresent>,
}

impl MonitorDoorsReceivers {
    pub fn from_receivers(receivers: &Receivers) -> Self {
        Self {
            frunk: receivers.frunk.clone(),
            boot: receivers.boot.clone(),
            doors: receivers.doors.clone(),
            windows: receivers.windows.clone(),
            user_present: receivers.user_present.clone(),
        }
    }
}

pub fn monitor_doors(state: &InitState, tesla: &Config, receivers: MonitorDoorsReceivers) {
    let message_sink = state.message_sink.clone();

    let (tx, rx) = stateful::create_pipe("tesla_doors");

    let tesla_clone = tesla.clone();
    spawn(async move {
        let mut frunk_s = receivers.frunk.subscribe().await;
        let mut boot_s = receivers.boot.subscribe().await;
        let mut doors_s = receivers.doors.subscribe().await;
        let mut windows_s = receivers.windows.subscribe().await;
        let mut user_present_s = receivers.user_present.subscribe().await;
        let name = &tesla_clone.name;

        loop {
            select! {
                Ok(_) = frunk_s.recv() => {},
                Ok(_) = boot_s.recv() => {},
                Ok(_) = doors_s.recv() => {},
                Ok(_) = windows_s.recv() => {},
                Ok(_) = user_present_s.recv() => {},
                else => break,
            };

            let mut open: Vec<Door> = vec![];

            let maybe_user_present = receivers.user_present.get().await;
            if Some(UserIsPresent::UserNotPresent) == maybe_user_present {
                let maybe_frunk = receivers.frunk.get().await;
                let maybe_boot = receivers.boot.get().await;
                let maybe_doors = receivers.doors.get().await;
                let maybe_windows = receivers.windows.get().await;

                debug!(
                    "{name}: fo: {:?}, to: {:?}, do: {:?}, wo: {:?}, up: {:?}",
                    maybe_frunk, maybe_boot, maybe_doors, maybe_windows, maybe_user_present
                );

                if Some(DoorState::Open) == maybe_frunk {
                    open.push(Door::Frunk);
                }

                if Some(DoorState::Open) == maybe_boot {
                    open.push(Door::Boot);
                }

                if Some(DoorState::Open) == maybe_doors {
                    open.push(Door::Doors);
                }

                // Ignore windows for now, as Tesla often reporting these are open when they are not.
                // if let Some(TeslaDoorState::Open) = maybe_wo {
                //     open.push(Door::Windows)
                // }
            } else {
                debug!("{name}: up: {:?}", maybe_user_present);
            }

            debug!("{name}: open: {:?}", open);
            tx.try_send(open);
        }
    });

    // We only care if doors open for at least 120 seconds.
    let duration = Duration::from_secs(120);
    let rx = delay_input(
        "tesla_doors (delayed)",
        duration,
        rx,
        |(_, c)| !c.is_empty(),
        DelayInputOptions {
            skip_subsequent_delay: true,
        },
    );

    // Discard initial [] value and duplicate events.
    let rx = rx.filter(|(p, c)| p.is_some() || !c.is_empty());

    // Repeat the last value every 5 minutes.
    let duration = Duration::from_secs(300);
    let rx = delay_repeat("tesla_doors (repeat)", duration, rx, |(_, c)| !c.is_empty());

    // Output the message.
    let tesla = tesla.clone();
    spawn(async move {
        let mut s = rx.subscribe().await;
        while let Ok(open) = s.recv().await {
            debug!("open received: {:?}", open);
            let msg = doors_to_message(&tesla, &open);
            let msg = new_message(msg, MessagePriority::Urgent);
            message_sink.try_send(msg);
        }
    });
}

fn doors_to_message(tesla: &Config, open: &[Door]) -> String {
    let name = &tesla.name;

    let msg = match open {
        [] => format!("{name} is secure"),
        // The Tesla doors are open
        [doors] if doors.is_plural() => {
            format!("{name} {doors} are open")
        }
        // The Tesla frunk is open
        [door] if !door.is_plural() => {
            format!("{name} {door} is open")
        }
        // The Tesla frunk and boot are open
        // The Tesla frunk, boot and doors are open
        // The Tesla doors, boot and frunk are open
        [doors @ .., last] => {
            let doors = doors
                .iter()
                .map(Door::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name} {doors} and {last} are open")
        }
    };
    msg
}

const SHORT_INTERVAL: Duration = Duration::from_secs(30);
const LONG_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[derive(Serialize, Deserialize, Debug, Default)]
struct PersistentState {
    auto_charge: bool,
}

/// Errors that can occur when monitoring charging.
#[derive(Debug, Error)]
pub enum MonitorChargingError {
    /// An error occurred when loading the persistent state.
    #[error("failed to load persistent state: {0}")]
    LoadPersistentState(#[from] persistent_state::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChargingMessage {
    Disconnected,
    Charging { limit: u8 },
    NoPower,
    Complete,
    Stopped,
}

impl ChargingMessage {
    const fn get(charging_info: &ChargingInformation) -> Self {
        let limit = charging_info.charge_limit;

        match charging_info.charging_state {
            ChargingStateEnum::Disconnected => Self::Disconnected,
            ChargingStateEnum::Charging | ChargingStateEnum::Starting => Self::Charging { limit },
            ChargingStateEnum::NoPower => Self::NoPower,
            ChargingStateEnum::Complete => Self::Complete,
            ChargingStateEnum::Stopped => Self::Stopped,
        }
    }

    fn to_string(self, level: u8) -> String {
        match self {
            Self::Disconnected => format!("is disconnected at {level}%"),
            Self::Charging { limit } => {
                format!("is charging from {level}% to {limit}%")
            }
            Self::NoPower => format!("plug failed at {level}%"),
            Self::Complete => format!("is finished charging at {level}%"),
            Self::Stopped => format!("has stopped charging at {level}%"),
        }
    }
}

fn announce_charging_state(
    tesla: &Config,
    old_charging_info: &ChargingInformation,
    charging_info: &ChargingInformation,
    message_sink: &stateless::Sender<Message>,
) {
    let name = &tesla.name;

    let plugged_in_msg = {
        let was_plugged_in = old_charging_info.charging_state.is_plugged_in();
        let is_plugged_in = charging_info.charging_state.is_plugged_in();

        if was_plugged_in && !is_plugged_in {
            Some("has been freed".to_string())
        } else if !was_plugged_in && is_plugged_in {
            Some("has been leashed".to_string())
        } else {
            None
        }
    };

    let charge_msg = {
        // We do not want an announcement every time the battery level changes.
        let level = charging_info.battery_level;
        // But we do want an announcement if other charging information changes.
        let old_msg = ChargingMessage::get(old_charging_info);
        let new_msg = ChargingMessage::get(charging_info);
        if old_msg == new_msg {
            None
        } else {
            new_msg.to_string(level).pipe(Some)
        }
    };

    if plugged_in_msg.is_some() || charge_msg.is_some() {
        let msg = [plugged_in_msg, charge_msg]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" and ");

        let msg = format!("{name} {msg}");
        let msg = new_message(msg, MessagePriority::DaytimeOnly);
        message_sink.try_send(msg);
    }
}

#[derive(Debug)]
struct TeslaState {
    charge_limit: u8,
    battery_level: u8,
    charging_state: ChargingStateEnum,
    is_at_home: bool,
    last_success: DateTime<Utc>,
    notified_errors: bool,
    send_left_home_commands: bool,
}

impl TeslaState {
    const fn is_charging(&self) -> bool {
        self.charging_state.is_charging()
    }
}

pub async fn check_token(
    token: &mut Token,
    tesla_secret: &PersistentStateRow<Token>,
) -> Result<(), TokenError> {
    info!("Refreshing state, token expiration: {:?}", token.expires_at);
    token.check(tesla_secret).await?;
    info!("Token expiration: {:?}", token.expires_at);
    Ok(())
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ChargingInformation {
    battery_level: u8,
    charge_limit: u8,
    charge_request_at_home: ChargeRequest,
    // charge_request: ChargeRequest,
    charging_state: ChargingStateEnum,
}

pub struct MonitorChargingReceivers {
    pub charge_request: stateful::Receiver<ChargeRequest>,
    pub is_home: stateful::Receiver<bool>,
    pub auto_charge: stateless::Receiver<Json<Command>>,
    pub charging_state: stateful::Receiver<ChargingStateEnum>,
    pub battery_level: stateful::Receiver<Parsed<u8>>,
    pub charge_limit: stateful::Receiver<Parsed<u8>>,
}

impl MonitorChargingReceivers {
    pub fn from_receivers(
        receivers: &Receivers,
        charge_request: stateful::Receiver<ChargeRequest>,
        is_home: stateful::Receiver<bool>,
    ) -> Self {
        Self {
            charge_request,
            is_home,
            auto_charge: receivers.auto_charge.clone(),
            charging_state: receivers.charging_state.clone(),
            battery_level: receivers.battery_level.clone(),
            charge_limit: receivers.charge_limit.clone(),
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn monitor_charging(
    state: &InitState,
    config: &Config,
    receivers: MonitorChargingReceivers,
) -> Result<stateful::Receiver<ChargingInformation>, MonitorChargingError> {
    let id = config.teslamate_id.to_string();

    let (tx_summary, rx_summary) = stateful::create_pipe("tesla_charging_summary");

    let tesla_secret = state.persistent_state_database.for_name("tesla_token");

    let psr = state
        .persistent_state_database
        .for_name::<PersistentState>(&format!("tesla_{id}"));
    let ps = psr.load().unwrap_or_default();

    let mqtt = state.mqtt.clone();
    let message_sink = state.message_sink.clone();

    let auto_charge_rx = {
        let mqtt = mqtt.clone();
        let teslamate_id = config.teslamate_id;

        receivers.auto_charge.map(move |Json(cmd)| {
            if let Command::Device(cmd) = &cmd {
                let status = match cmd.action {
                    DeviceAction::TurnOn => DevicePower::AutoOff,
                    DeviceAction::TurnOff => DevicePower::Off,
                };
                publish_auto_charge(teslamate_id, status, &mqtt);
            }
            cmd
        })
    };

    let mut token = Token::get(&tesla_secret)?;

    let config = config.clone();
    spawn(async move {
        let name = &config.name;

        match check_token(&mut token, &tesla_secret).await {
            Ok(()) => {}
            Err(err) => {
                error!("{name}: Failed to check token: {}", err);
            }
        }

        if let Err(err) = test_tesla_api(&token, config.tesla_id).await {
            error!("{name}: Failed to talk to Tesla API: {err}");
        };

        let mut charge_request_s = receivers.charge_request.subscribe().await;
        let mut auto_charge_s = auto_charge_rx.subscribe().await;
        let mut is_home_s = receivers.is_home.subscribe().await;
        let mut battery_level_s = receivers.battery_level.subscribe().await;
        let mut charge_limit_s = receivers.charge_limit.subscribe().await;
        let mut charging_state_s = receivers.charging_state.subscribe().await;

        let mut amber_charge_request: ChargeRequest = charge_request_s
            .recv()
            .await
            .unwrap_or(ChargeRequest::ChargeTo(0));
        let mut ps = ps;

        let mut tesla_state = TeslaState {
            charge_limit: *charge_limit_s.recv().await.as_deref().unwrap_or(&0),
            battery_level: *battery_level_s.recv().await.as_deref().unwrap_or(&0),
            charging_state: charging_state_s
                .recv()
                .await
                .unwrap_or(ChargingStateEnum::Disconnected),
            is_at_home: is_home_s.recv().await.unwrap_or(false),
            last_success: Utc::now(),
            notified_errors: false,
            send_left_home_commands: false,
        };

        info!("{name}: Initial Tesla state: {:?}", tesla_state);

        tx_summary.try_send(ChargingInformation {
            battery_level: tesla_state.battery_level,
            charging_state: tesla_state.charging_state,
            charge_limit: tesla_state.charge_limit,
            charge_request_at_home: should_charge_at_home(&ps, amber_charge_request),
            // charge_request: should_charge_maybe_at_home(
            //     tesla_state.is_at_home,
            //     &ps,
            //     amber_charge_request,
            // ),
        });

        let mut timer = {
            let new_interval = SHORT_INTERVAL;
            info!("{name}: Next poll {}", duration::to_string(&new_interval));
            tokio::time::Instant::now().add(new_interval)
        };

        loop {
            let was_at_home = tesla_state.is_at_home;

            select! {
                () = sleep_until(timer) => {
                    match check_token(&mut token, &tesla_secret).await {
                        Ok(()) => {}
                        Err(err) => {
                            error!("{name}: Failed to check token: {}", err);
                        }
                    }
                }
                Ok(new_charge_request) = charge_request_s.recv() => {
                    info!("{name}: New price summary: {:?}", new_charge_request);
                    amber_charge_request = new_charge_request;
                }
                Ok(cmd) = auto_charge_s.recv() => {
                    if let Command::Device(cmd) = cmd {
                        ps.auto_charge = match cmd.action {
                            DeviceAction::TurnOn => true,
                            DeviceAction::TurnOff => false,
                        };
                        psr.save(&ps).unwrap_or_else(|e| {
                            error!("{name}: Error saving persistent state: {}", e);
                        });
                        info!("{name}: Auto charge: {}", ps.auto_charge);
                        update_auto_charge(ps.auto_charge,config.teslamate_id, &tesla_state, &mqtt);
                    } else {
                        info!("{name}: Ignoring invalid auto_charge command: {cmd:?}");
                    }
                }
                Ok(Parsed(new_charge_limit)) = charge_limit_s.recv() => {
                    info!("{name}: Charge limit: {new_charge_limit}");
                    tesla_state.charge_limit = new_charge_limit;
                }
                Ok(Parsed(new_charge_level)) = battery_level_s.recv() => {
                    info!("{name}: Charge level: {new_charge_level}");
                    tesla_state.battery_level = new_charge_level;
                }
                Ok(new_is_at_home) = is_home_s.recv() => {
                    info!("{name}: Location is at home: {new_is_at_home}");
                    tesla_state.is_at_home = new_is_at_home;
                }

                Ok(charging_state) = charging_state_s.recv() => {
                    info!("{name}: Charging state: {charging_state:?}");
                    tesla_state.charging_state = charging_state;
                }
            }

            let is_at_home = tesla_state.is_at_home;
            if was_at_home && !is_at_home {
                info!("{name}: left home - sending left home commands");
                tesla_state.send_left_home_commands = true;
            } else if is_at_home && tesla_state.send_left_home_commands {
                info!("{name}: at home - cancelling left home commands");
                tesla_state.send_left_home_commands = false;
            }

            let result = if tesla_state.send_left_home_commands && amber_charge_request.is_auto() {
                // Construct sequence of commands to send to Tesla.
                let mut sequence = CommandSequence::new();

                // Set charging limit to 90% when car leaves home.
                sequence.add_wake_up();
                sequence.add_set_chart_limit(90);

                // Send the commands.
                info!("{name}: Sending left home commands: {sequence:?}");
                sequence.execute(&token, config.tesla_id).await
            } else {
                let charge_request =
                    should_charge_maybe_at_home(is_at_home, &ps, amber_charge_request);
                check_charge(&config, &token, &tesla_state, charge_request).await
            };

            tx_summary.try_send(ChargingInformation {
                battery_level: tesla_state.battery_level,
                charge_limit: tesla_state.charge_limit,
                charging_state: tesla_state.charging_state,
                charge_request_at_home: should_charge_at_home(&ps, amber_charge_request),
                // charge_request,
            });

            let new_interval = match result {
                Ok(()) => {
                    info!("{name}: Success executing command sequence");
                    notify_success(&tesla_state, &message_sink);
                    forget_errors(&mut tesla_state);
                    LONG_INTERVAL
                }
                Err(SequenceError::WaitRetry(duration)) => {
                    info!("{name}: Failed, retrying in {:?}", duration);
                    notify_errors(&mut tesla_state, &message_sink);
                    duration
                }
                Err(err) => {
                    info!("{name}: Error executing command sequence: {}", err);
                    notify_errors(&mut tesla_state, &message_sink);
                    SHORT_INTERVAL
                }
            };

            info!("{name}: Next poll {}", duration::to_string(&new_interval));
            timer = tokio::time::Instant::now().add(new_interval);

            update_auto_charge(ps.auto_charge, config.teslamate_id, &tesla_state, &mqtt);
        }
    });

    Ok(rx_summary)
}

const fn should_charge_maybe_at_home(
    is_at_home: bool,
    ps: &PersistentState,
    amber_charge_request: ChargeRequest,
) -> ChargeRequest {
    if is_at_home && ps.auto_charge {
        amber_charge_request
    } else {
        ChargeRequest::Manual
    }
}

const fn should_charge_at_home(
    ps: &PersistentState,
    amber_charge_request: ChargeRequest,
) -> ChargeRequest {
    if ps.auto_charge {
        amber_charge_request
    } else {
        ChargeRequest::Manual
    }
}

fn forget_errors(tesla_state: &mut TeslaState) {
    tesla_state.last_success = Utc::now();
    tesla_state.notified_errors = false;
    tesla_state.send_left_home_commands = false;
}

fn notify_success(tesla_state: &TeslaState, message_sink: &stateless::Sender<Message>) {
    if tesla_state.notified_errors {
        let msg = new_message(
            "I am on talking terms with the Tesla again",
            MessagePriority::Urgent,
        );
        message_sink.try_send(msg);
    }
}

const FAILURE_NOTIFICATION_INTERVAL: TimeDelta = unsafe_time_delta!(minutes: 30);

fn notify_errors(tesla_state: &mut TeslaState, message_sink: &stateless::Sender<Message>) {
    if !tesla_state.notified_errors
        && tesla_state.last_success.add(FAILURE_NOTIFICATION_INTERVAL) < Utc::now()
    {
        let msg = new_message(
            "The Tesla and I have not been talking to each other for 30 minutes",
            MessagePriority::Urgent,
        );
        message_sink.try_send(msg);
        tesla_state.notified_errors = true;
        // If we have been trying to send left home commands for 30 minutes, then give up.
        tesla_state.send_left_home_commands = false;
    }
}

fn update_auto_charge(
    auto_charge: bool,
    teslamate_id: TeslamateId,
    tesla_state: &TeslaState,
    mqtt: &robotica_backend::services::mqtt::MqttTx,
) {
    let notified_errors = tesla_state.notified_errors;
    let is_charging = tesla_state.is_charging();
    let status = match (notified_errors, auto_charge, is_charging) {
        (true, _, _) => DevicePower::DeviceError,
        (false, true, false) => DevicePower::AutoOff,
        (false, true, true) => DevicePower::On,
        (false, false, _) => DevicePower::Off,
    };

    publish_auto_charge(teslamate_id, status, mqtt);
}

fn publish_auto_charge(
    teslamate_id: TeslamateId,
    status: DevicePower,
    mqtt: &robotica_backend::services::mqtt::MqttTx,
) {
    let topic = format!(
        "state/Tesla/{id}/AutoCharge/power",
        id = teslamate_id.to_string()
    );
    let string: String = status.into();
    let msg = MqttMessage::new(topic, string, Retain::Retain, QoS::AtLeastOnce);
    mqtt.try_send(msg);
}

async fn test_tesla_api(token: &Token, tesla_id: VehicleId) -> Result<()> {
    let data = token.get_vehicles().await?;

    data.into_iter()
        .find(|vehicle| vehicle.id == tesla_id)
        .ok_or_else(|| {
            anyhow::anyhow!("Tesla vehicle {id} not found", id = tesla_id.to_string())
        })?;

    Ok(())
}

enum ChargingSummary {
    Charging,
    NotCharging,
    Disconnected,
}

#[allow(clippy::too_many_lines)]
async fn check_charge(
    tesla: &Config,
    token: &Token,
    tesla_state: &TeslaState,
    charge_request: ChargeRequest,
) -> Result<(), SequenceError> {
    info!("Checking charge");
    let tesla_id = tesla.tesla_id;
    let name = &tesla.name;

    let (should_charge, charge_limit) = should_charge(charge_request, tesla_state);

    // We should not attempt to start charging if charging is complete.
    let charging_state = tesla_state.charging_state;
    let can_start_charge = charging_state != ChargingStateEnum::Complete;

    info!(
        "{name}: Current data: {charge_request:?}, {tesla_state:?}, notified_errors: {}",
        tesla_state.notified_errors
    );
    info!("{name}: Desired State: should charge: {should_charge:?}, can start charge: {can_start_charge}, charge limit: {charge_limit}");

    // Do we need to set the charge limit?
    let set_charge_limit =
        should_charge != ShouldCharge::DontTouch && tesla_state.charge_limit != charge_limit;

    // Construct sequence of commands to send to Tesla.
    let mut sequence = CommandSequence::new();

    // Wake up the car if it's not already awake.
    sequence.add_wake_up();

    // Set the charge limit if required.
    // Or if we are in error state
    // - we need to keep checking to find out when the car is awake.
    if set_charge_limit || tesla_state.notified_errors {
        info!("{name}: Setting charge limit to {}", charge_limit);
        sequence.add_set_chart_limit(charge_limit);
    }

    // Get charging state
    #[allow(clippy::match_same_arms)]
    let charging_summary = match charging_state {
        ChargingStateEnum::Starting => ChargingSummary::Charging,
        ChargingStateEnum::Charging => ChargingSummary::Charging,
        ChargingStateEnum::Complete => ChargingSummary::NotCharging,
        ChargingStateEnum::Stopped => ChargingSummary::NotCharging,
        ChargingStateEnum::Disconnected => ChargingSummary::Disconnected,
        ChargingStateEnum::NoPower => ChargingSummary::Disconnected,
    };

    // Start/stop charging as required.
    {
        use ShouldCharge::DoCharge;
        use ShouldCharge::DoNotCharge;
        #[allow(clippy::match_same_arms)]
        match charging_summary {
            ChargingSummary::Charging if should_charge == DoNotCharge => {
                info!("{name}: Stopping charge");
                sequence.add_charge_stop();
            }
            ChargingSummary::Charging => {}
            ChargingSummary::NotCharging if should_charge == DoCharge && can_start_charge => {
                info!("{name}: Starting charge");
                sequence.add_charge_start();
            }
            ChargingSummary::NotCharging => {}
            ChargingSummary::Disconnected => info!("{name}: is disconnected"),
        }
    }

    // Send the commands.
    info!("{name}: Sending commands: {sequence:?}");
    let result = sequence.execute(token, tesla_id).await.map_err(|err| {
        info!("{name}: Error executing command sequence: {}", err);
        err
    });

    // If we attempted to change anything, ensure teslamate is logging so we get updates.
    if !sequence.is_empty() {
        // Any errors here should be logged and forgotten.
        if let Err(err) = enable_teslamate_logging(tesla).await {
            error!("{name}: Failed to enable teslamate logging: {}", err);
        }
    }

    info!("{name}: All done. {result:?}");
    result
}

#[derive(Debug, Error)]
enum TeslamateError {
    #[error("Failed to enable logging: {0}")]
    Error(#[from] reqwest::Error),

    #[error("Failed to parse teslamate url: {0}")]
    ParseError(#[from] url::ParseError),
}

async fn enable_teslamate_logging(config: &Config) -> Result<(), TeslamateError> {
    let url = config.teslamate.url.join("/api/car/1/logging/resume")?;
    let client = reqwest::Client::new().put(url);
    let client = match &config.teslamate.auth {
        TeslamateAuth::Basic { username, password } => client.basic_auth(username, Some(password)),
        TeslamateAuth::None => client,
    };
    client.send().await?.error_for_status()?;
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
enum ShouldCharge {
    DoCharge,
    DoNotCharge,
    DontTouch,
}

fn should_charge(charge_request: ChargeRequest, tesla_state: &TeslaState) -> (ShouldCharge, u8) {
    let (should_charge, charge_limit) = match charge_request {
        // RequestedCharge::DontCharge => (ShouldCharge::DoNotCharge, 50),
        ChargeRequest::ChargeTo(limit) => (ShouldCharge::DoCharge, limit),
        ChargeRequest::Manual => (ShouldCharge::DontTouch, tesla_state.charge_limit),
    };

    #[allow(clippy::match_same_arms)]
    let should_charge = match (should_charge, tesla_state.battery_level) {
        (sc @ ShouldCharge::DoCharge, level) => {
            if level < charge_limit {
                sc
            } else {
                ShouldCharge::DoNotCharge
            }
        }
        (sc @ ShouldCharge::DoNotCharge, _) => sc,
        (sc @ ShouldCharge::DontTouch, _) => sc,
    };

    let charge_limit = charge_limit.clamp(50, 90);
    (should_charge, charge_limit)
}
