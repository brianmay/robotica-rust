use crate::amber::car::ChargeRequest;
use crate::audience;
use crate::delays::{delay_input, delay_repeat, DelayInputOptions};

use anyhow::Result;
use chrono::{DateTime, TimeDelta, Timelike, Utc};
use robotica_backend::services::persistent_state::{self, PersistentStateRow};
use robotica_backend::services::tesla::api::{
    ChargingStateEnum, CommandSequence, SequenceError, Token, TokenError, VehicleId,
};
use robotica_common::datetime::duration;
use robotica_common::robotica::audio::MessagePriority;
use robotica_common::robotica::commands::Command;
use robotica_common::robotica::locations::{self, LocationMessage};
use robotica_common::robotica::message::Message;
use robotica_common::robotica::switch::{DeviceAction, DevicePower};
use robotica_common::{robotica, teslamate, unsafe_time_delta};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::ops::Add;
use std::time::Duration;
use tap::Pipe;
use thiserror::Error;
use tokio::select;
use tokio::time::sleep_until;
use tracing::{debug, error, info};

use robotica_backend::pipes::{stateful, stateless, Subscriber, Subscription};
use robotica_backend::{database, spawn};
use robotica_common::mqtt::{BoolError, Json, MqttMessage, Parsed, QoS, Retain};

use super::InitState;

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct TeslamateId(u32);

impl ToString for TeslamateId {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub teslamate_id: TeslamateId,
    pub tesla_id: VehicleId,
}

fn new_message(message: impl Into<String>, priority: MessagePriority) -> Message {
    Message::new("Tesla", message.into(), priority, audience::everyone())
}

fn new_private_message(message: impl Into<String>, priority: MessagePriority) -> Message {
    Message::new("Tesla", message.into(), priority, audience::brian(true))
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TeslaDoorState {
    Open,
    Closed,
}

impl Display for TeslaDoorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::Closed => write!(f, "closed"),
        }
    }
}

impl TryFrom<MqttMessage> for TeslaDoorState {
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
enum TeslaUserIsPresent {
    UserPresent,
    UserNotPresent,
}

impl Display for TeslaUserIsPresent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserPresent => write!(f, "user is present"),
            Self::UserNotPresent => write!(f, "user is not present"),
        }
    }
}

impl TryFrom<MqttMessage> for TeslaUserIsPresent {
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

pub fn monitor_teslamate_location(
    state: &mut InitState,
    postgres: sqlx::PgPool,
    tesla: &Config,
) -> stateful::Receiver<robotica::locations::LocationMessage> {
    let (tx, rx) = stateful::create_pipe("teslamate_location");
    let id = tesla.teslamate_id.to_string();
    let mqtt = state.mqtt.clone();

    let inputs = state
        .subscriptions
        .subscribe_into_stateful::<Json<teslamate::Location>>(&format!(
            "teslamate/cars/{id}/location"
        ));

    spawn(async move {
        let mut inputs = inputs.subscribe().await;

        while let Ok(Json(location)) = inputs.recv().await {
            let point = geo::Point::new(location.longitude, location.latitude);
            let locations = database::locations::search_locations(&postgres, point)
                .await
                .unwrap_or_else(|err| {
                    error!("Failed to search locations: {}", err);
                    vec![]
                });
            let output = robotica::locations::LocationMessage {
                position: point,
                locations,
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

    rx
}

pub fn monitor_tesla_location(
    state: &InitState,
    location_stream: stateful::Receiver<LocationMessage>,
    charging_info: stateful::Receiver<ChargingInformation>,
) -> stateful::Receiver<ShouldPlugin> {
    let message_sink = state.message_sink.clone();
    let (tx, rx) = stateful::create_pipe("tesla_should_plugin");

    spawn(async move {
        let mut location_s = location_stream.subscribe().await;
        let mut charging_info_s = charging_info.subscribe().await;

        let Ok(mut old_location) = location_s.recv().await else {
            error!("Failed to get initial Tesla location");
            return;
        };
        let mut old_is_at_home = old_location.is_at_home();

        let Ok(mut old_charging_info) = charging_info_s.recv().await else {
            error!("Failed to get initial Tesla charging information");
            return;
        };

        debug!("Initial Tesla location: {:?}", old_location);
        debug!(
            "Initial Tesla charging information: {:?}",
            old_charging_info
        );

        loop {
            select! {
                Ok(new_charging_info) = charging_info_s.recv() => {
                    if old_location.is_at_home()  {
                        announce_charging_state(&old_charging_info, &new_charging_info, &message_sink);
                    }
                    old_charging_info = new_charging_info;
                },
                Ok(new_location) = location_s.recv() => {
                    let new_is_at_home = new_location.is_at_home();

                    let old_map: HashMap<i32, &locations::Location> = old_location.locations.iter().map(|l| (l.id, l)).collect();
                    let new_map: HashMap<i32, &locations::Location> = new_location.locations.iter().map(|l| (l.id, l)).collect();
                    let old_set: HashSet<i32> = old_location.locations.iter().map(|l| l.id).collect();
                    let new_set: HashSet<i32> = new_location.locations.iter().map(|l| l.id).collect();

                    new_set.difference(&old_set).for_each(|id| {
                        if let Some(location) = new_map.get(id)  {
                            let msg = format!("The Tesla arrived in {}", location.name);
                            let msg = if location.announce_on_enter {
                                new_message(msg, MessagePriority::Low)
                            } else {
                                new_private_message(msg, MessagePriority::Low)
                            };
                            message_sink.try_send(msg);
                        }
                    });

                    old_set.difference(&new_set).for_each(|id| {
                        if let Some(location) = old_map.get(id)  {
                            let msg = format!("The Tesla left {}", location.name);
                            let msg = if location.announce_on_exit {
                                new_message(msg, MessagePriority::Low)
                            } else {
                                new_private_message(msg, MessagePriority::Low)
                            };
                            message_sink.try_send(msg);
                        }
                    });

                    if !old_is_at_home && new_is_at_home {
                        let level = old_charging_info.battery_level;
                        let ChargeRequest::ChargeTo(limit) = old_charging_info.charge_request;
                        let msg = if level < limit {
                            format!("The Tesla is at {level}% and would charge to {limit}%")
                        } else {
                            format!("The Tesla is at {level}% and the limit is {limit}%")
                        };
                        let msg = new_message(msg, MessagePriority::DaytimeOnly);
                        message_sink.try_send(msg);
                    }

                    old_location = new_location;
                    old_is_at_home = new_is_at_home;
                }
                else => break,
            }

            let should_plugin = if old_is_at_home
                && !old_charging_info.charging_state.is_plugged_in()
                && !old_charging_info.battery_level <= 80
            {
                ShouldPlugin::ShouldPlugin
            } else {
                ShouldPlugin::NoActionRequired
            };
            tx.try_send(should_plugin);
        }
    });

    rx
}

pub fn plug_in_reminder(state: &InitState, should_plugin_stream: stateful::Receiver<ShouldPlugin>) {
    let message_sink = state.message_sink.clone();

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
                let msg = new_message(
                    "The Tesla might run away and should be leashed",
                    MessagePriority::Low,
                );
                message_sink.try_send(msg);
            }
        }
    });
}

pub fn monitor_tesla_doors(state: &mut InitState, tesla: &Config) {
    let id = tesla.teslamate_id.to_string();

    let frunk_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaDoorState>(&format!("teslamate/cars/{id}/frunk_open"));
    let boot_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaDoorState>(&format!("teslamate/cars/{id}/trunk_open"));
    let doors_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaDoorState>(&format!("teslamate/cars/{id}/doors_open"));
    let windows_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaDoorState>(&format!("teslamate/cars/{id}/windows_open"));
    let user_present_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaUserIsPresent>(&format!(
            "teslamate/cars/{id}/is_user_present"
        ));

    let message_sink = state.message_sink.clone();

    let (tx, rx) = stateful::create_pipe("tesla_doors");

    spawn(async move {
        let mut frunk_s = frunk_rx.subscribe().await;
        let mut boot_s = boot_rx.subscribe().await;
        let mut doors_s = doors_rx.subscribe().await;
        let mut windows_s = windows_rx.subscribe().await;
        let mut user_present_s = user_present_rx.subscribe().await;

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

            let maybe_user_present = user_present_rx.get().await;
            if Some(TeslaUserIsPresent::UserNotPresent) == maybe_user_present {
                let maybe_frunk = frunk_rx.get().await;
                let maybe_boot = boot_rx.get().await;
                let maybe_doors = doors_rx.get().await;
                let maybe_windows = windows_rx.get().await;

                debug!(
                    "fo: {:?}, to: {:?}, do: {:?}, wo: {:?}, up: {:?}",
                    maybe_frunk, maybe_boot, maybe_doors, maybe_windows, maybe_user_present
                );

                if Some(TeslaDoorState::Open) == maybe_frunk {
                    open.push(Door::Frunk);
                }

                if Some(TeslaDoorState::Open) == maybe_boot {
                    open.push(Door::Boot);
                }

                if Some(TeslaDoorState::Open) == maybe_doors {
                    open.push(Door::Doors);
                }

                // Ignore windows for now, as Tesla often reporting these are open when they are not.
                // if let Some(TeslaDoorState::Open) = maybe_wo {
                //     open.push(Door::Windows)
                // }
            } else {
                debug!("up: {:?}", maybe_user_present);
            }

            debug!("open: {:?}", open);
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
    spawn(async move {
        let mut s = rx.subscribe().await;
        while let Ok(open) = s.recv().await {
            debug!("open received: {:?}", open);
            let msg = doors_to_message(&open);
            let msg = new_message(msg, MessagePriority::Urgent);
            message_sink.try_send(msg);
        }
    });
}

fn doors_to_message(open: &[Door]) -> String {
    let msg = match open {
        [] => "The Tesla is secure".to_string(),
        // The Tesla doors are open
        [doors] if doors.is_plural() => {
            format!("The Tesla {doors} are open")
        }
        // The Tesla frunk is open
        [door] if !door.is_plural() => {
            format!("The Tesla {door} is open")
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
            format!("The Tesla {doors} and {last} are open")
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
        let ChargeRequest::ChargeTo(limit) = charging_info.charge_request;

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
    old_charging_info: &ChargingInformation,
    charging_info: &ChargingInformation,
    message_sink: &stateless::Sender<Message>,
) {
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

        let msg = format!("The Tesla {msg}");
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

enum TeslaResult {
    Skipped,
    Tried(Result<(), SequenceError>),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ChargingInformation {
    battery_level: u8,
    charge_request: ChargeRequest,
    charging_state: ChargingStateEnum,
}

#[allow(clippy::too_many_lines)]
pub fn monitor_charging(
    state: &mut InitState,
    tesla: &Config,
    charge_request_rx: stateful::Receiver<ChargeRequest>,
) -> Result<stateful::Receiver<ChargingInformation>, MonitorChargingError> {
    let id = tesla.teslamate_id.to_string();

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
        let teslamate_id = tesla.teslamate_id;

        state
            .subscriptions
            .subscribe_into_stateless::<Json<Command>>(&format!("command/Tesla/{id}/AutoCharge"))
            .map(move |Json(cmd)| {
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

    let is_home_rx = {
        state
            .subscriptions
            .subscribe_into_stateful::<String>(&format!(
                "state/Tesla/{id}/Location",
                id = tesla.teslamate_id.to_string()
            ))
            .map(move |(_, location)| location == "home")
    };

    let charging_state_rx = state
        .subscriptions
        .subscribe_into_stateful::<ChargingStateEnum>(&format!(
            "teslamate/cars/{id}/charging_state"
        ));

    let battery_level = state
        .subscriptions
        .subscribe_into_stateful::<Parsed<u8>>(&format!("teslamate/cars/{id}/battery_level"));

    let charge_limit = state
        .subscriptions
        .subscribe_into_stateful::<Parsed<u8>>(&format!("teslamate/cars/{id}/charge_limit_soc"));

    let mut token = Token::get(&tesla_secret)?;

    let tesla = tesla.clone();
    spawn(async move {
        match check_token(&mut token, &tesla_secret).await {
            Ok(()) => {}
            Err(err) => {
                error!("Failed to check token: {}", err);
            }
        }

        if let Err(err) = test_tesla_api(&token, tesla.tesla_id).await {
            error!("Failed to talk to Tesla API: {}", err);
        };

        let mut charge_request_s = charge_request_rx.subscribe().await;
        let mut auto_charge_s = auto_charge_rx.subscribe().await;
        let mut is_home_s = is_home_rx.subscribe().await;
        let mut battery_level_s = battery_level.subscribe().await;
        let mut charge_limit_s = charge_limit.subscribe().await;
        let mut charging_state_s = charging_state_rx.subscribe().await;

        let mut charge_request: ChargeRequest = charge_request_s
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

        info!("Initial Tesla state: {:?}", tesla_state);

        tx_summary.try_send(ChargingInformation {
            battery_level: tesla_state.battery_level,
            charging_state: tesla_state.charging_state,
            charge_request,
        });

        let mut timer = {
            let new_interval = SHORT_INTERVAL;
            info!("Next poll {}", duration::to_string(&new_interval));
            tokio::time::Instant::now().add(new_interval)
        };

        loop {
            let was_at_home = tesla_state.is_at_home;

            select! {
                () = sleep_until(timer) => {
                    match check_token(&mut token, &tesla_secret).await {
                        Ok(()) => {}
                        Err(err) => {
                            error!("Failed to check token: {}", err);
                        }
                    }
                }
                Ok(new_charge_request) = charge_request_s.recv() => {
                    info!("New price summary: {:?}", new_charge_request);
                    charge_request = new_charge_request;
                }
                Ok(cmd) = auto_charge_s.recv() => {
                    if let Command::Device(cmd) = cmd {
                        ps.auto_charge = match cmd.action {
                            DeviceAction::TurnOn => true,
                            DeviceAction::TurnOff => false,
                        };
                        psr.save(&ps).unwrap_or_else(|e| {
                            error!("Error saving persistent state: {}", e);
                        });
                        info!("Auto charge: {}", ps.auto_charge);
                        update_auto_charge(ps.auto_charge,tesla.teslamate_id, &tesla_state, &mqtt);
                    } else {
                        info!("Ignoring invalid auto_charge command: {cmd:?}");
                    }
                }
                Ok(Parsed(new_charge_limit)) = charge_limit_s.recv() => {
                    info!("Charge limit: {new_charge_limit}");
                    tesla_state.charge_limit = new_charge_limit;
                }
                Ok(Parsed(new_charge_level)) = battery_level_s.recv() => {
                    info!("Charge level: {new_charge_level}");
                    tesla_state.battery_level = new_charge_level;
                }
                Ok(new_is_at_home) = is_home_s.recv() => {
                    info!("Location is at home: {new_is_at_home}");
                    tesla_state.is_at_home = new_is_at_home;
                }

                Ok(charging_state) = charging_state_s.recv() => {
                    info!("Charging state: {charging_state:?}");
                    tesla_state.charging_state = charging_state;
                }
            }

            let is_at_home = tesla_state.is_at_home;
            if was_at_home && !is_at_home {
                info!("Tesla has left home - sending left home commands");
                tesla_state.send_left_home_commands = true;
            } else if is_at_home && tesla_state.send_left_home_commands {
                info!("Tesla is at home - cancelling left home commands");
                tesla_state.send_left_home_commands = false;
            }

            let result = if tesla_state.send_left_home_commands {
                // Construct sequence of commands to send to Tesla.
                let mut sequence = CommandSequence::new();

                // Set charging limit to 90% when car leaves home.
                sequence.add_wake_up();
                sequence.add_set_chart_limit(90);

                // Send the commands.
                info!("Sending left home commands: {sequence:?}");
                let result = sequence.execute(&token, tesla.tesla_id).await;
                TeslaResult::Tried(result)
            } else if is_at_home && ps.auto_charge {
                let result =
                    check_charge(tesla.tesla_id, &token, &tesla_state, charge_request).await;
                TeslaResult::Tried(result)
            } else {
                info!(
                    "Skipping charge check, is_at_home={is_at_home:?}, auto_charge={auto_charge:?}",
                    auto_charge = ps.auto_charge
                );
                TeslaResult::Skipped
            };

            tx_summary.try_send(ChargingInformation {
                battery_level: tesla_state.battery_level,
                charging_state: tesla_state.charging_state,
                charge_request,
            });

            let new_interval = match result {
                TeslaResult::Skipped => {
                    // If we skipped, then lets just pretend we succeeded.
                    forget_errors(&mut tesla_state);
                    LONG_INTERVAL
                }
                TeslaResult::Tried(Ok(())) => {
                    info!("Success executing command sequence");
                    notify_success(&tesla_state, &message_sink);
                    forget_errors(&mut tesla_state);
                    LONG_INTERVAL
                }
                TeslaResult::Tried(Err(SequenceError::WaitRetry(duration))) => {
                    info!("Failed, retrying in {:?}", duration);
                    notify_errors(&mut tesla_state, &message_sink);
                    duration
                }
                TeslaResult::Tried(Err(err)) => {
                    info!("Error executing command sequence: {}", err);
                    notify_errors(&mut tesla_state, &message_sink);
                    SHORT_INTERVAL
                }
            };

            info!("Next poll {}", duration::to_string(&new_interval));
            timer = tokio::time::Instant::now().add(new_interval);

            update_auto_charge(ps.auto_charge, tesla.teslamate_id, &tesla_state, &mqtt);
        }
    });

    Ok(rx_summary)
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
    tesla_id: VehicleId,
    token: &Token,
    tesla_state: &TeslaState,
    charge_request: ChargeRequest,
) -> Result<(), SequenceError> {
    info!("Checking charge");

    let (should_charge, charge_limit) = should_charge(charge_request, tesla_state);

    // We should not attempt to start charging if charging is complete.
    let charging_state = tesla_state.charging_state;
    let can_start_charge = charging_state != ChargingStateEnum::Complete;

    info!(
        "Current data: {charge_request:?}, {tesla_state:?}, notified_errors: {}",
        tesla_state.notified_errors
    );
    info!("Desired State: should charge: {should_charge:?}, can start charge: {can_start_charge}, charge limit: {charge_limit}");

    // Do we need to set the charge limit?
    let set_charge_limit = tesla_state.charge_limit != charge_limit;

    // Construct sequence of commands to send to Tesla.
    let mut sequence = CommandSequence::new();

    // Wake up the car if it's not already awake.
    sequence.add_wake_up();

    // Set the charge limit if required.
    // Or if we are in error state
    // - we need to keep checking to find out when the car is awake.
    if set_charge_limit || tesla_state.notified_errors {
        info!("Setting charge limit to {}", charge_limit);
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
                info!("Stopping charge");
                sequence.add_charge_stop();
            }
            ChargingSummary::Charging => {}
            ChargingSummary::NotCharging if should_charge == DoCharge && can_start_charge => {
                info!("Starting charge");
                sequence.add_charge_start();
            }
            ChargingSummary::NotCharging => {}
            ChargingSummary::Disconnected => info!("Car is disconnected"),
        }
    }

    // Send the commands.
    info!("Sending commands: {sequence:?}");
    let result = sequence.execute(token, tesla_id).await.map_err(|err| {
        info!("Error executing command sequence: {}", err);
        err
    });

    info!("All done. {result:?}");
    result
}

#[derive(Debug, Eq, PartialEq)]
enum ShouldCharge {
    DoCharge,
    DoNotCharge,
    // DontTouch,
}

fn should_charge(charge_request: ChargeRequest, tesla_state: &TeslaState) -> (ShouldCharge, u8) {
    let (should_charge, charge_limit) = match charge_request {
        // RequestedCharge::DontCharge => (ShouldCharge::DoNotCharge, 50),
        ChargeRequest::ChargeTo(limit) => (ShouldCharge::DoCharge, limit),
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
        // (sc @ ShouldCharge::DontTouch, _) => sc,
    };

    let charge_limit = charge_limit.clamp(50, 90);
    (should_charge, charge_limit)
}
