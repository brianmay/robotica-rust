use crate::amber::car::ChargeRequest;
use crate::audience;
use crate::delays::{delay_input, delay_repeat, DelayInputOptions};

use anyhow::Result;
use chrono::{DateTime, Utc};
use robotica_backend::services::persistent_state::{self, PersistentStateRow};
use robotica_backend::services::tesla::api::{
    ChargingStateEnum, CommandSequence, SequenceError, Token, TokenError,
};
use robotica_common::robotica::audio::MessagePriority;
use robotica_common::robotica::commands::Command;
use robotica_common::robotica::message::Message;
use robotica_common::robotica::switch::{DeviceAction, DevicePower};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::ops::Add;
use std::time::Duration;
use thiserror::Error;
use tokio::select;
use tokio::time::Interval;
use tracing::{debug, error, info};

use robotica_backend::pipes::{stateful, stateless, Subscriber, Subscription};
use robotica_backend::spawn;
use robotica_common::mqtt::{BoolError, Json, MqttMessage, Parsed, QoS};

use super::InitState;

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

#[derive(Debug, Eq, PartialEq)]
enum Location {
    String(String),
    Nowhere,
}

impl Location {
    fn from_string(s: String) -> Self {
        if s == "not_home" {
            Self::Nowhere
        } else {
            Self::String(s)
        }
    }

    fn censor(&self) -> bool {
        match self {
            Self::String(s) => s != "home",
            Self::Nowhere => false,
        }
    }
}

pub fn monitor_tesla_location(state: &mut InitState, car_number: usize) {
    let location = state
        .subscriptions
        .subscribe_into_stateful::<String>(&format!("state/Tesla/{car_number}/Location"));

    let duration = Duration::from_secs(30);
    let location = delay_input(
        "tesla_location",
        duration,
        location,
        |(old_location, location)| old_location.is_some() && location != "not_home",
        DelayInputOptions::default(),
    );
    let message_sink = state.message_sink.clone();

    spawn(async move {
        let mut location_s = location.subscribe().await;
        let mut old_location = {
            let Ok(old_location_raw) = location_s.recv().await else {
                error!("Failed to get initial Tesla location");
                return;
            };
            Location::from_string(old_location_raw)
        };

        while let Ok(new_location_raw) = location_s.recv().await {
            let new_location = Location::from_string(new_location_raw);

            // Departed location message.
            let msg = left_location_message(&old_location);
            output_location_message(&old_location, msg, &message_sink);

            // Arrived location message.
            let msg = arrived_location_message(&new_location);
            output_location_message(&new_location, msg, &message_sink);

            old_location = new_location;
        }
    });
}

fn output_location_message(
    location: &Location,
    msg: Option<String>,
    message_sink: &stateless::Sender<Message>,
) {
    let msg = if location.censor() {
        msg.map(|msg| new_private_message(msg, MessagePriority::Low))
    } else {
        msg.map(|msg| new_message(msg, MessagePriority::Low))
    };
    if let Some(msg) = msg {
        message_sink.try_send(msg);
    };
}

fn left_location_message(location: &Location) -> Option<String> {
    match location {
        Location::String(location) => Some(format!("The Tesla left {location}")),
        Location::Nowhere => None,
    }
}

fn arrived_location_message(location: &Location) -> Option<String> {
    match location {
        Location::String(location) => Some(format!("The Tesla arrived at {location}")),
        Location::Nowhere => None,
    }
}

pub fn monitor_tesla_doors(state: &mut InitState, car_number: usize) {
    let frunk_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaDoorState>(&format!(
            "teslamate/cars/{car_number}/frunk_open"
        ));
    let boot_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaDoorState>(&format!(
            "teslamate/cars/{car_number}/trunk_open"
        ));
    let doors_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaDoorState>(&format!(
            "teslamate/cars/{car_number}/doors_open"
        ));
    let windows_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaDoorState>(&format!(
            "teslamate/cars/{car_number}/windows_open"
        ));
    let user_present_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaUserIsPresent>(&format!(
            "teslamate/cars/{car_number}/is_user_present"
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PollInterval {
    Short,
    Long,
}

impl From<PollInterval> for Duration {
    fn from(pi: PollInterval) -> Self {
        match pi {
            PollInterval::Short => Self::from_secs(30),
            PollInterval::Long => Self::from_secs(5 * 60),
        }
    }
}

impl From<PollInterval> for Interval {
    fn from(pi: PollInterval) -> Self {
        let duration: Duration = pi.into();
        tokio::time::interval(duration)
    }
}

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

fn announce_charging_state(
    charging_state: ChargingStateEnum,
    old_tesla_state: &TeslaState,
    message_sink: &stateless::Sender<Message>,
) {
    let was_plugged_in = old_tesla_state
        .charging_state
        .map(ChargingStateEnum::is_plugged_in);
    let is_plugged_in = charging_state.is_plugged_in();

    #[allow(clippy::bool_comparison)]
    let plugged_in_msg = if was_plugged_in == Some(true) && is_plugged_in == false {
        Some("has been freed".to_string())
    } else if was_plugged_in == Some(false) && is_plugged_in == true {
        Some("has been leashed".to_string())
    } else {
        None
    };

    let charge_msg = match charging_state {
        ChargingStateEnum::Disconnected => "is disconnected",
        ChargingStateEnum::Charging => "is charging",
        ChargingStateEnum::NoPower => "plug failed",
        ChargingStateEnum::Complete => "is finished charging",
        ChargingStateEnum::Starting => "is starting to charge",
        ChargingStateEnum::Stopped => "has stopped charging",
    };

    let charge_msg = old_tesla_state.battery_level.map_or_else(
        || charge_msg.to_string(),
        |level| format!("{charge_msg} at ({level}%)"),
    );

    let msg = [plugged_in_msg, Some(charge_msg)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" and ");

    let msg = format!("The Tesla {msg}");
    let msg = new_message(msg, MessagePriority::DaytimeOnly);
    message_sink.try_send(msg);
}

#[derive(Debug)]
struct TeslaState {
    charge_limit: Option<u8>,
    battery_level: Option<u8>,
    charging_state: Option<ChargingStateEnum>,
    is_at_home: Option<bool>,
    last_success: DateTime<Utc>,
    notified_errors: bool,
    send_left_home_commands: bool,
}

impl TeslaState {
    fn is_charging(&self) -> bool {
        self.charging_state
            .map_or(false, ChargingStateEnum::is_charging)
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

#[allow(clippy::too_many_lines)]
pub fn monitor_charging(
    state: &mut InitState,
    car_number: usize,
    charge_request_rx: stateful::Receiver<ChargeRequest>,
) -> Result<(), MonitorChargingError> {
    let tesla_secret = state.persistent_state_database.for_name("tesla_token");

    let psr = state
        .persistent_state_database
        .for_name::<PersistentState>(&format!("tesla_{car_number}"));
    let ps = psr.load().unwrap_or_default();

    let mqtt = state.mqtt.clone();
    let message_sink = state.message_sink.clone();

    let auto_charge_rx = {
        let mqtt = mqtt.clone();
        state
            .subscriptions
            .subscribe_into_stateless::<Json<Command>>(&format!(
                "command/Tesla/{car_number}/AutoCharge"
            ))
            .map(move |Json(cmd)| {
                if let Command::Device(cmd) = &cmd {
                    let status = match cmd.action {
                        DeviceAction::TurnOn => DevicePower::AutoOff,
                        DeviceAction::TurnOff => DevicePower::Off,
                    };
                    publish_auto_charge(car_number, status, &mqtt);
                }
                cmd
            })
    };

    let is_home_rx = {
        state
            .subscriptions
            .subscribe_into_stateful::<String>(&format!("state/Tesla/{car_number}/Location"))
            .map(move |(_, location)| location == "home")
    };

    let charging_state_rx = state
        .subscriptions
        .subscribe_into_stateful::<ChargingStateEnum>(&format!(
            "teslamate/cars/{car_number}/charging_state"
        ));

    let battery_level = state
        .subscriptions
        .subscribe_into_stateful::<Parsed<u8>>(&format!(
            "teslamate/cars/{car_number}/battery_level"
        ));

    let charge_limit = state
        .subscriptions
        .subscribe_into_stateful::<Parsed<u8>>(&format!(
            "teslamate/cars/{car_number}/charge_limit_soc"
        ));

    let mut token = Token::get(&tesla_secret)?;

    spawn(async move {
        match check_token(&mut token, &tesla_secret).await {
            Ok(()) => {}
            Err(err) => {
                error!("Failed to check token: {}", err);
            }
        }

        let car_id = match get_car_id(&token, car_number).await {
            Ok(Some(car_id)) => car_id,
            Ok(None) => {
                error!("No car ID found for car number {}", car_number);
                return;
            }
            Err(err) => {
                error!("Failed to get car ID: {}", err);
                return;
            }
        };

        let mut tesla_state = TeslaState {
            charge_limit: None,
            battery_level: None,
            charging_state: None,
            is_at_home: None,
            last_success: Utc::now(),
            notified_errors: false,
            send_left_home_commands: false,
        };

        let mut interval = PollInterval::Long;
        let mut timer: Interval = interval.into();

        let mut charge_request_s = charge_request_rx.subscribe().await;
        let mut auto_charge_s = auto_charge_rx.subscribe().await;
        let mut is_home_s = is_home_rx.subscribe().await;
        let mut battery_level_s = battery_level.subscribe().await;
        let mut charge_limit_s = charge_limit.subscribe().await;
        let mut charging_state_s = charging_state_rx.subscribe().await;

        let mut charge_request: Option<ChargeRequest> = None;
        let mut ps = ps;

        loop {
            let was_at_home = tesla_state.is_at_home;

            select! {
                _ = timer.tick() => {
                    match check_token(&mut token, &tesla_secret).await {
                        Ok(()) => {}
                        Err(err) => {
                            error!("Failed to check token: {}", err);
                        }
                    }
                }
                Ok(new_charge_request) = charge_request_s.recv() => {
                    info!("New price summary: {:?}", new_charge_request);
                    charge_request = Some(new_charge_request);
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
                        update_auto_charge(ps.auto_charge, car_number, &tesla_state, &mqtt);
                    } else {
                        info!("Ignoring invalid auto_charge command: {cmd:?}");
                    }
                }
                Ok(Parsed(new_charge_limit)) = charge_limit_s.recv() => {
                    info!("Charge limit: {new_charge_limit}");
                    tesla_state.charge_limit = Some(new_charge_limit);
                }
                Ok(Parsed(new_charge_level)) = battery_level_s.recv() => {
                    info!("Charge level: {new_charge_level}");
                    tesla_state.battery_level = Some(new_charge_level);
                }
                Ok(new_is_at_home) = is_home_s.recv() => {
                    info!("Location is at home: {new_is_at_home}");
                    tesla_state.is_at_home = Some(new_is_at_home);
                }

                Ok((old, charging_state)) = charging_state_s.recv_old_new() => {
                    info!("Charging state: {charging_state:?}");
                    if old.is_some() {
                        announce_charging_state(charging_state, &tesla_state, &message_sink);
                    }
                    tesla_state.charging_state = Some(charging_state);
                }
            }

            let is_at_home = tesla_state.is_at_home;
            if was_at_home == Some(true) && is_at_home == Some(false) {
                info!("Tesla has left home - sending left home commands");
                tesla_state.send_left_home_commands = true;
            } else if is_at_home == Some(true) && tesla_state.send_left_home_commands {
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
                let result = sequence.execute(&token, car_id).await;
                TeslaResult::Tried(result)
            } else if is_at_home == Some(true) && ps.auto_charge {
                if let Some(charge_request) = charge_request {
                    let result = check_charge(car_id, &token, &tesla_state, charge_request).await;
                    TeslaResult::Tried(result)
                } else {
                    info!("No price summary available, skipping charge check");
                    TeslaResult::Skipped
                }
            } else {
                info!(
                    "Skipping charge check, is_at_home={is_at_home:?}, auto_charge={auto_charge:?}",
                    auto_charge = ps.auto_charge
                );
                TeslaResult::Skipped
            };

            let new_interval = match result {
                TeslaResult::Skipped => {
                    // If we skipped, then lets just pretend we succeeded.
                    tesla_state.last_success = Utc::now();
                    tesla_state.notified_errors = false;
                    tesla_state.send_left_home_commands = false;
                    PollInterval::Long
                }
                TeslaResult::Tried(Ok(())) => {
                    info!("Success executing command sequence");
                    notify_success(&tesla_state, &message_sink);
                    tesla_state.last_success = Utc::now();
                    tesla_state.notified_errors = false;
                    tesla_state.send_left_home_commands = false;
                    PollInterval::Long
                }
                TeslaResult::Tried(Err(err)) => {
                    info!("Error executing command sequence: {}", err);
                    notify_errors(&mut tesla_state, &message_sink);
                    PollInterval::Short
                }
            };

            if interval != new_interval {
                interval = new_interval;
                timer = interval.into();
                info!("Resetting poll timer to {interval:?} {:?}", timer.period());
            }

            info!("Next poll {interval:?} {:?}", timer.period());

            update_auto_charge(ps.auto_charge, car_number, &tesla_state, &mqtt);
        }
    });

    Ok(())
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

fn notify_errors(tesla_state: &mut TeslaState, message_sink: &stateless::Sender<Message>) {
    if !tesla_state.notified_errors
        && tesla_state.last_success.add(chrono::Duration::minutes(30)) < Utc::now()
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
    car_number: usize,
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

    publish_auto_charge(car_number, status, mqtt);
}

fn publish_auto_charge(
    car_number: usize,
    status: DevicePower,
    mqtt: &robotica_backend::services::mqtt::MqttTx,
) {
    let topic = format!("state/Tesla/{car_number}/AutoCharge/power");
    let string: String = status.into();
    let msg = MqttMessage::new(topic, string, true, QoS::AtLeastOnce);
    mqtt.try_send(msg);
}

async fn get_car_id(token: &Token, car_n: usize) -> Result<Option<u64>> {
    let vehicles = token.get_vehicles().await?;
    let vehicle = vehicles.get(car_n - 1);
    debug!("Got vehicle: {:?}", vehicle);
    let number = vehicle.map(|v| v.id);
    Ok(number)
}

enum ChargingSummary {
    Charging,
    NotCharging,
    Disconnected,
    Unknown,
}

#[allow(clippy::too_many_lines)]
async fn check_charge(
    car_id: u64,
    token: &Token,
    tesla_state: &TeslaState,
    charge_request: ChargeRequest,
) -> Result<(), SequenceError> {
    info!("Checking charge");

    let (should_charge, charge_limit) = should_charge(charge_request, tesla_state);

    // We should not attempt to start charging if charging is complete.
    let charging_state = &tesla_state.charging_state;
    let can_start_charge =
        charging_state.map_or(true, |state| state != ChargingStateEnum::Complete);

    info!(
        "Current data: {charge_request:?}, {tesla_state:?}, notified_errors: {}",
        tesla_state.notified_errors
    );
    info!("Desired State: should charge: {should_charge:?}, can start charge: {can_start_charge}, charge limit: {charge_limit}");

    // Do we need to set the charge limit?
    let set_charge_limit = tesla_state
        .charge_limit
        .map_or(true, |current_limit| current_limit != charge_limit);

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
        Some(ChargingStateEnum::Starting) => ChargingSummary::Charging,
        Some(ChargingStateEnum::Charging) => ChargingSummary::Charging,
        Some(ChargingStateEnum::Complete) => ChargingSummary::NotCharging,
        Some(ChargingStateEnum::Stopped) => ChargingSummary::NotCharging,
        Some(ChargingStateEnum::Disconnected) => ChargingSummary::Disconnected,
        Some(ChargingStateEnum::NoPower) => ChargingSummary::Disconnected,
        None => ChargingSummary::Unknown,
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
            ChargingSummary::Unknown if should_charge == DoNotCharge => {
                info!("Stopping charge (unknown)");
                sequence.add_charge_stop();
            }
            ChargingSummary::Unknown if should_charge == DoCharge && can_start_charge => {
                info!("Starting charge (unknown)");
                sequence.add_charge_start();
            }
            ChargingSummary::Unknown => {}
            ChargingSummary::Disconnected => info!("Car is disconnected"),
        }
    }

    // Send the commands.
    info!("Sending commands: {sequence:?}");
    let result = sequence.execute(token, car_id).await.map_err(|err| {
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
        (sc @ ShouldCharge::DoCharge, Some(level)) => {
            if level < charge_limit {
                sc
            } else {
                ShouldCharge::DoNotCharge
            }
        }
        (sc @ ShouldCharge::DoCharge, None) => sc,
        (sc @ ShouldCharge::DoNotCharge, _) => sc,
        // (sc @ ShouldCharge::DontTouch, _) => sc,
    };

    let charge_limit = charge_limit.clamp(50, 90);
    (should_charge, charge_limit)
}
