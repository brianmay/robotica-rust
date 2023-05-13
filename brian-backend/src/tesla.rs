use crate::amber::{PriceCategory, PriceSummary};
use crate::delays::{delay_input, delay_repeat};

use anyhow::Result;
use robotica_backend::services::persistent_state;
use robotica_backend::services::tesla::api::{ChargingStateEnum, CommandSequence, Token};
use robotica_common::robotica::audio::{Message, MessagePriority};
use robotica_common::robotica::commands::Command;
use robotica_common::robotica::switch::{DeviceAction, DevicePower};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::time::Duration;
use thiserror::Error;
use tokio::select;
use tokio::time::Interval;
use tracing::{debug, error, info};

use robotica_backend::entities::{create_stateless_entity, StatelessReceiver, StatelessSender};
use robotica_backend::spawn;
use robotica_common::mqtt::{BoolError, Json, MqttMessage, Parsed, QoS};

use super::State;

fn new_message(message: impl Into<String>, priority: MessagePriority) -> Message {
    Message::new("Tesla", message.into(), priority)
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
    const fn to_str(&self) -> &'static str {
        match self {
            Self::Frunk => "frunk",
            Self::Boot => "boot",
            Self::Doors => "doors",
            Self::Windows => "windows",
        }
    }
}

pub fn monitor_tesla_doors(state: &mut State, car_number: usize) {
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

    let (tx, rx) = create_stateless_entity("tesla_doors");

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
    let rx = delay_input("tesla_doors (delayed)", duration, rx, |c| !c.is_empty());

    // Discard initial [] value and duplicate events.
    let rx = rx
        .map_stateful(|f| f)
        .filter(|(p, c)| p.is_some() || !c.is_empty());

    // Repeat the last value every 5 minutes.
    let duration = Duration::from_secs(300);
    let rx = delay_repeat("tesla_doors (repeat)", duration, rx, |(_, c)| !c.is_empty());

    // Output the message.
    spawn(async move {
        let mut s = rx.subscribe().await;
        while let Ok(open) = s.recv().await {
            debug!("open received: {:?}", open);
            let open = open.iter().map(Door::to_str).collect::<Vec<_>>();
            let msg = if open.is_empty() {
                "The Tesla is secure".to_string()
            } else if open.len() == 1 {
                format!("The Tesla {} is open", open.join(", "))
            } else {
                format!("The Tesla {} are open", open.join(", "))
            };
            let msg = new_message(msg, MessagePriority::Urgent);
            message_sink.try_send(msg);
        }
    });
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
    force_charge: bool,
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
    tesla_state: &TeslaState,
    message_sink: &StatelessSender<Message>,
) {
    let msg = match charging_state {
        ChargingStateEnum::Disconnected => "The Tesla is disconnected",
        ChargingStateEnum::Charging => "The Tesla is charging",
        ChargingStateEnum::NoPower => "The Tesla plug failed",
        ChargingStateEnum::Complete => "The Tesla is finished charging",
        ChargingStateEnum::Starting => "The Tesla is starting to charge",
        ChargingStateEnum::Stopped => "The Tesla has stopped charging",
    };

    let msg = tesla_state
        .battery_level
        .map_or_else(|| msg.to_string(), |level| format!("{msg} at ({}%)", level));

    let msg = new_message(msg, MessagePriority::DaytimeOnly);
    message_sink.try_send(msg);
}

#[derive(Debug)]
struct TeslaState {
    charge_limit: Option<u8>,
    battery_level: Option<u8>,
    charging_state: Option<ChargingStateEnum>,
    is_home: Option<bool>,
}

impl TeslaState {
    fn is_charging(&self) -> bool {
        self.charging_state.map_or(false, |cs| cs.is_charging())
    }
}

#[allow(clippy::too_many_lines)]
pub fn monitor_charging(
    state: &mut State,
    car_number: usize,
    price_summary_rx: StatelessReceiver<PriceSummary>,
) -> Result<(), MonitorChargingError> {
    let tesla_secret = state.persistent_state_database.for_name("tesla_token");

    let psr = state
        .persistent_state_database
        .for_name::<PersistentState>(&format!("tesla_{car_number}"));
    let ps = psr.load().unwrap_or_default();

    let mqtt = state.mqtt.clone();
    let message_sink = state.message_sink.clone();

    let price_category_rx = price_summary_rx.map_stateful(|ps| ps.category);

    let auto_charge_rx = {
        let mqtt = mqtt.clone();
        state
            .subscriptions
            .subscribe_into::<Json<Command>>(&format!("command/Tesla/{car_number}/AutoCharge"))
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

    let force_charge_rx = {
        let mqtt = mqtt.clone();
        state
            .subscriptions
            .subscribe_into::<Json<Command>>(&format!("command/Tesla/{car_number}/ForceCharge"))
            .map(move |Json(cmd)| {
                if let Command::Device(cmd) = &cmd {
                    let status = match cmd.action {
                        DeviceAction::TurnOn => DevicePower::AutoOff,
                        DeviceAction::TurnOff => DevicePower::Off,
                    };
                    publish_force_change(car_number, status, &mqtt);
                }
                cmd
            })
    };

    let is_home_rx = {
        state
            .subscriptions
            .subscribe_into::<String>(&format!("state/Tesla/{car_number}/Location"))
            .map_stateful(move |location| location == "home")
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
        let car_id = match get_car_id(&mut token, car_number).await {
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
            is_home: None,
        };

        let mut interval = PollInterval::Long;
        let mut timer: Interval = interval.into();

        let mut price_category_s = price_category_rx.subscribe().await;
        let mut auto_charge_s = auto_charge_rx.subscribe().await;
        let mut force_charge_s = force_charge_rx.subscribe().await;
        let mut location_charge_s = is_home_rx.subscribe().await;
        let mut battery_level_s = battery_level.subscribe().await;
        let mut charge_limit_s = charge_limit.subscribe().await;
        let mut charging_state_s = charging_state_rx.subscribe().await;

        // let mut charge_state = None;
        let mut price_category: Option<PriceCategory> = None;
        let mut ps = ps;
        let mut is_home = false;

        // info!("Initial charge state: {charge_state:?}");

        loop {
            select! {
                _ = timer.tick() => {
                    info!("Refreshing state, token expiration: {:?}", token.expires_at);
                    token.check(&tesla_secret).await.unwrap_or_else(|e| {
                        error!("Error refreshing token: {}", e);
                    });
                    info!("Token expiration: {:?}", token.expires_at);
                }
                Ok(new_price_category) = price_category_s.recv() => {
                    info!("New price summary: {:?}", new_price_category);
                    price_category = Some(new_price_category);
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
                Ok(cmd) = force_charge_s.recv() => {
                    if let Command::Device(cmd) = cmd {
                        ps.force_charge = match cmd.action {
                            DeviceAction::TurnOn => true,
                            DeviceAction::TurnOff => false,
                        };
                        psr.save(&ps).unwrap_or_else(|e| {
                            error!("Error saving persistent state: {}", e);
                        });
                        info!("Force charge: {}", ps.force_charge);
                        update_force_charge(&tesla_state, car_number, ps.force_charge, &mqtt);
                    } else {
                        info!("Ignoring invalid force_charge command: {cmd:?}");
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
                Ok(new_is_home) = location_charge_s.recv() => {
                    is_home = new_is_home;
                    info!("Location is home: {is_home}");
                    tesla_state.is_home = Some(is_home);
                }

                Ok((old, charging_state)) = charging_state_s.recv_value() => {
                    info!("Charging state: {charging_state:?}");
                    if old.is_some() {
                        announce_charging_state(charging_state, &tesla_state, &message_sink);
                    }
                    tesla_state.charging_state = Some(charging_state);
                }
            }

            let new_interval = if is_home && ps.auto_charge {
                if let Some(price_category) = price_category {
                    let result = check_charge(
                        car_id,
                        &token,
                        &tesla_state,
                        price_category,
                        ps.force_charge,
                    )
                    .await;
                    match result {
                        Ok(()) => PollInterval::Long,
                        Err(CheckChargeError::ScheduleRetry) => PollInterval::Short,
                    }
                } else {
                    info!("No price summary available, skipping charge check");
                    PollInterval::Long
                }
            } else {
                info!("Skipping charge check");
                PollInterval::Long
            };

            if interval != new_interval {
                interval = new_interval;
                timer = interval.into();
                info!("Resetting poll timer to {interval:?} {:?}", timer.period());
            }

            info!("Next poll {interval:?} {:?}", timer.period());

            update_auto_charge(ps.auto_charge, car_number, &tesla_state, &mqtt);
            update_force_charge(&tesla_state, car_number, ps.force_charge, &mqtt);
        }
    });

    Ok(())
}

fn update_auto_charge(
    auto_charge: bool,
    car_number: usize,
    tesla_state: &TeslaState,
    mqtt: &robotica_backend::services::mqtt::MqttTx,
) {
    let is_charging = tesla_state.is_charging();
    let status = match (auto_charge, is_charging) {
        (true, false) => DevicePower::AutoOff,
        (true, true) => DevicePower::On,
        (false, _) => DevicePower::Off,
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

fn update_force_charge(
    tesla_state: &TeslaState,
    car_number: usize,
    force_charge: bool,
    mqtt: &robotica_backend::services::mqtt::MqttTx,
) {
    let is_charging = tesla_state.is_charging();
    let status = match (force_charge, is_charging) {
        (true, false) => DevicePower::AutoOff,
        (true, true) => DevicePower::On,
        (false, _) => DevicePower::Off,
    };

    publish_force_change(car_number, status, mqtt);
}

fn publish_force_change(
    car_number: usize,
    status: DevicePower,
    mqtt: &robotica_backend::services::mqtt::MqttTx,
) {
    let topic = format!("state/Tesla/{car_number}/ForceCharge/power");
    let string: String = status.into();
    let msg = MqttMessage::new(topic, string, true, QoS::AtLeastOnce);
    mqtt.try_send(msg);
}

async fn get_car_id(token: &mut Token, car_n: usize) -> Result<Option<u64>> {
    let vehicles = token.get_vehicles().await?;
    let vehicle = vehicles.get(car_n - 1);
    debug!("Got vehicle: {:?}", vehicle);
    let number = vehicle.map(|v| v.id);
    Ok(number)
}

enum RequestedCharge {
    DontCharge,
    ChargeTo(u8),
}

enum ChargingSummary {
    Charging,
    NotCharging,
    Disconnected,
    Unknown,
}

impl RequestedCharge {
    const fn min_charge(self, min_charge: u8) -> Self {
        match self {
            Self::DontCharge => Self::ChargeTo(min_charge),
            Self::ChargeTo(limit) if limit < min_charge => Self::ChargeTo(min_charge),
            Self::ChargeTo(_) => self,
        }
    }
}

#[derive(Error, Debug)]
enum CheckChargeError {
    #[error("Error getting charge state")]
    ScheduleRetry,
}

#[allow(clippy::too_many_lines)]
async fn check_charge(
    car_id: u64,
    token: &Token,
    tesla_state: &TeslaState,
    price_category: PriceCategory,
    force_charge: bool,
) -> Result<(), CheckChargeError> {
    info!("Checking charge");
    let mut rc_err = None;

    let (should_charge, charge_limit) = should_charge(price_category, force_charge, tesla_state);

    // We should not attempt to start charging if charging is complete.
    let charging_state = &tesla_state.charging_state;
    let can_start_charge = match charging_state {
        Some(state) => *state != ChargingStateEnum::Complete,
        None => true,
    };

    info!("Current data: {price_category:?}, {tesla_state:?}, force charge: {force_charge}");
    info!("Desired State: should charge: {should_charge}, can start charge: {can_start_charge}, charge limit: {charge_limit}");

    // Do we need to set the charge limit?
    let set_charge_limit = if let Some(current_limit) = tesla_state.charge_limit {
        current_limit != charge_limit
    } else {
        true
    };

    // Construct sequence of commands to send to Tesla.
    let mut sequence = CommandSequence::new();

    // Wake up the car if it's not already awake.
    sequence.add_wake_up();

    // Set the charge limit if required.
    if set_charge_limit {
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
    #[allow(clippy::match_same_arms)]
    match charging_summary {
        ChargingSummary::Charging if !should_charge => {
            info!("Stopping charge");
            sequence.add_charge_stop();
        }
        ChargingSummary::Charging => {}
        ChargingSummary::NotCharging if should_charge && can_start_charge => {
            info!("Starting charge");
            sequence.add_charge_start();
        }
        ChargingSummary::NotCharging => {}
        ChargingSummary::Unknown if !should_charge => {
            info!("Stopping charge (unknown)");
            sequence.add_charge_stop();
        }
        ChargingSummary::Unknown if should_charge && can_start_charge => {
            info!("Starting charge (unknown)");
            sequence.add_charge_start();
        }
        ChargingSummary::Unknown => {}
        ChargingSummary::Disconnected => info!("Car is disconnected"),
    }

    // Send the commands.
    info!("Sending commands: {sequence:?}");
    sequence.execute(token, car_id).await.unwrap_or_else(|err| {
        info!("Error executing command sequence: {}", err);
        rc_err = Some(CheckChargeError::ScheduleRetry);
    });

    // Generate result.
    let result = match rc_err {
        // Something went wrong above, we should retry.
        Some(err) => Err(err),

        // Everything is OK. Supposedly.
        None => Ok(()),
    };

    info!("All done. {result:?}");
    result
}

const fn should_charge(
    price_category: PriceCategory,
    force_charge: bool,
    tesla_state: &TeslaState,
) -> (bool, u8) {
    #[allow(clippy::match_same_arms)]
    let requested_charge = match &price_category {
        PriceCategory::Expensive => RequestedCharge::DontCharge,
        PriceCategory::Normal => RequestedCharge::DontCharge,
        PriceCategory::Cheap => RequestedCharge::ChargeTo(80),
        PriceCategory::SuperCheap => RequestedCharge::ChargeTo(90),
    };
    let requested_charge = if force_charge {
        requested_charge.min_charge(70)
    } else {
        requested_charge
    };
    let (should_charge, charge_limit) = match requested_charge {
        RequestedCharge::DontCharge => (false, 50),
        RequestedCharge::ChargeTo(limit) => (true, limit),
    };
    let should_charge = match tesla_state.battery_level {
        Some(level) => should_charge && level < charge_limit,
        None => should_charge,
    };
    (should_charge, charge_limit)
}
