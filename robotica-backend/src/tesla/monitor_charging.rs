use opentelemetry::{global, KeyValue};
use robotica_common::robotica::entities::Id;
use robotica_common::{
    mqtt::{Json, Parsed},
    robotica::{
        commands::Command,
        switch::{DeviceAction, DevicePower},
    },
};
use robotica_tokio::{
    pipes::{stateful, stateless, Subscriber, Subscription},
    services::{persistent_state, tesla::api::ChargingStateEnum},
    spawn,
};
use serde::{Deserialize, Serialize};
use tap::Pipe;
use thiserror::Error;
use tokio::select;
use tracing::{error, info};

use crate::{amber::car::ChargeRequest, car};

use super::{command_processor, ChargingInformation, Receivers};

#[derive(Debug)]
struct Meters {
    charging: opentelemetry::metrics::Gauge<u64>,
    battery: opentelemetry::metrics::Gauge<u64>,
    id: Id,
}

impl Meters {
    fn new(config: &car::Config) -> Self {
        let meter = global::meter("tesla::monitor_charging");

        Self {
            charging: meter.u64_gauge("charging").build(),
            battery: meter.u64_gauge("battery").build(),
            id: config.id.clone(),
        }
    }

    fn set_charging(&self, value: ChargingStateEnum, limit: u8) {
        let attributes = vec![KeyValue::new("id", self.id.to_string())];
        let value = if value.is_charging() { limit } else { 0 };
        self.charging.record(u64::from(value), &attributes);
    }

    fn set_battery(&self, value: u8) {
        let attributes = vec![KeyValue::new("id", self.id.to_string())];
        self.battery.record(u64::from(value), &attributes);
    }
}

/// Errors that can occur when monitoring charging.
#[derive(Debug, Error)]
pub enum Error {
    /// An error occurred when loading the persistent state.
    #[error("failed to load persistent state: {0}")]
    LoadPersistentState(#[from] persistent_state::Error),
}

pub struct Inputs {
    pub charge_request: stateful::Receiver<ChargeRequest>,
    pub is_home: stateful::Receiver<bool>,
    pub auto_charge: stateless::Receiver<Json<Command>>,
    pub charging_state: stateful::Receiver<ChargingStateEnum>,
    pub battery_level: stateful::Receiver<Parsed<u8>>,
    pub charge_limit: stateful::Receiver<Parsed<u8>>,
}

impl Inputs {
    pub fn from_receivers(
        receivers: &Receivers,
        charge_request: stateful::Receiver<ChargeRequest>,
        auto_charge: stateless::Receiver<Json<Command>>,
        is_home: stateful::Receiver<bool>,
    ) -> Self {
        Self {
            charge_request,
            is_home,
            auto_charge,
            charging_state: receivers.charging_state.clone(),
            battery_level: receivers.battery_level.clone(),
            charge_limit: receivers.charge_limit.clone(),
        }
    }
}

pub struct Outputs {
    pub charging_information: stateful::Receiver<ChargingInformation>,
    pub commands: stateless::Receiver<command_processor::Command>,
    pub auto_charge: stateful::Receiver<DevicePower>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct PersistentState {
    auto_charge: bool,
}

#[derive(Debug)]
struct TeslaState {
    charge_limit: u8,
    battery_level: u8,
    charging_state: ChargingStateEnum,
    is_at_home: bool,
}

impl TeslaState {
    const fn is_charging(&self) -> bool {
        self.charging_state.is_charging()
    }
}

#[allow(clippy::too_many_lines)]
pub fn monitor_charging(
    // state: &InitState,
    persistent_state_database: &persistent_state::PersistentStateDatabase,
    car: &car::Config,
    receivers: Inputs,
) -> Result<Outputs, Error> {
    let id = car.id.clone();

    let (tx_summary, rx_summary) = stateful::create_pipe("tesla_charging_summary");
    let (tx_command, rx_command) = stateless::create_pipe("tesla_charging_command");
    let (tx_auto_charge, rx_auto_charge) = stateful::create_pipe("tesla_auto_charge");

    let psr = persistent_state_database.for_name::<PersistentState>(&id, "tesla_monitor_charging");
    let ps = psr.load().unwrap_or_default();

    // let mqtt = state.mqtt.clone();
    let auto_charge_rx = receivers.auto_charge;

    let meters = Meters::new(car);

    let config = car.clone();
    spawn(async move {
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
        };

        info!(%id, "Initial Tesla state: {:?}", tesla_state);

        tx_summary.try_send(ChargingInformation {
            battery_level: tesla_state.battery_level,
            charging_state: tesla_state.charging_state,
            charge_limit: tesla_state.charge_limit,
            charge_request_at_home: should_charge_at_home(&ps, amber_charge_request),
        });

        meters.set_charging(tesla_state.charging_state, tesla_state.charge_limit);
        meters.set_battery(tesla_state.battery_level);

        loop {
            let was_at_home = tesla_state.is_at_home;

            select! {
                Ok(new_charge_request) = charge_request_s.recv() => {
                    info!(%id, "New price summary: {:?}", new_charge_request);
                    amber_charge_request = new_charge_request;
                }
                Ok(cmd) = auto_charge_s.recv() => {
                    if let Json(Command::Device(cmd)) = cmd {
                        ps.auto_charge = match cmd.action {
                            DeviceAction::TurnOn => true,
                            DeviceAction::TurnOff => false,
                        };
                        psr.save(&ps).unwrap_or_else(|e| {
                            error!(%id, "Error saving persistent state: {}", e);
                        });
                        info!(%id, "Auto charge: {}", ps.auto_charge);
                        update_auto_charge(ps.auto_charge, &tesla_state, &tx_auto_charge);
                    } else {
                        info!(%id, "Ignoring invalid auto_charge command: {cmd:?}");
                    }
                }
                Ok(Parsed(new_charge_limit)) = charge_limit_s.recv() => {
                    info!(%id, "Charge limit: {new_charge_limit}");
                    tesla_state.charge_limit = new_charge_limit;
                }
                Ok(Parsed(new_charge_level)) = battery_level_s.recv() => {
                    info!(%id, "Charge level: {new_charge_level}");
                    tesla_state.battery_level = new_charge_level;
                    meters.set_battery(tesla_state.battery_level);
                }
                Ok(new_is_at_home) = is_home_s.recv() => {
                    info!(%id, "Location is at home: {new_is_at_home}");
                    tesla_state.is_at_home = new_is_at_home;
                }

                Ok(charging_state) = charging_state_s.recv() => {
                    info!(%id, "Charging state: {charging_state:?}");
                    tesla_state.charging_state = charging_state;
                    meters.set_charging(tesla_state.charging_state, tesla_state.charge_limit);
                }
            }

            let is_at_home = tesla_state.is_at_home;
            let send_left_home_commands =
                was_at_home && !is_at_home && amber_charge_request.is_auto();

            if send_left_home_commands {
                let command = command_processor::Command::new().set_charge_limit(90);
                tx_command.try_send(command);
            } else {
                let charge_request =
                    should_charge_maybe_at_home(is_at_home, &ps, amber_charge_request);
                check_charge(&config, &tx_command, &tesla_state, charge_request);
            };

            tx_summary.try_send(ChargingInformation {
                battery_level: tesla_state.battery_level,
                charge_limit: tesla_state.charge_limit,
                charging_state: tesla_state.charging_state,
                charge_request_at_home: should_charge_at_home(&ps, amber_charge_request),
            });

            update_auto_charge(ps.auto_charge, &tesla_state, &tx_auto_charge);
        }
    });

    Outputs {
        charging_information: rx_summary,
        commands: rx_command,
        auto_charge: rx_auto_charge,
    }
    .pipe(Ok)
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

fn update_auto_charge(
    auto_charge: bool,
    tesla_state: &TeslaState,
    mqtt: &stateful::Sender<DevicePower>,
) {
    let notified_errors = false;
    let is_charging = tesla_state.is_charging();
    let status = match (notified_errors, auto_charge, is_charging) {
        (true, _, _) => DevicePower::DeviceError,
        (false, true, false) => DevicePower::AutoOff,
        (false, true, true) => DevicePower::On,
        (false, false, _) => DevicePower::Off,
    };

    mqtt.try_send(status);
}

enum ChargingSummary {
    Charging,
    NotCharging,
    Disconnected,
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
fn check_charge(
    car: &car::Config,
    tx: &stateless::Sender<command_processor::Command>,
    tesla_state: &TeslaState,
    charge_request: ChargeRequest,
) {
    let id = &car.id;

    info!("Checking charge");

    let (should_charge, charge_limit) = should_charge(charge_request, tesla_state);

    // We should not attempt to start charging if charging is complete.
    let charging_state = tesla_state.charging_state;
    let can_start_charge = charging_state != ChargingStateEnum::Complete;

    info!(%id, "Current data: {charge_request:?}, {tesla_state:?}",);
    info!(%id, "Desired State: should charge: {should_charge:?}, can start charge: {can_start_charge}, charge limit: {charge_limit}");

    // Do we need to set the charge limit?
    let set_charge_limit =
        should_charge != ShouldCharge::DontTouch && tesla_state.charge_limit != charge_limit;

    // Construct sequence of commands to send to Tesla.
    let mut sequence = command_processor::Command::new();

    // Set the charge limit if required.
    if set_charge_limit {
        info!(%id, "Setting charge limit to {}", charge_limit);
        sequence = sequence.set_charge_limit(charge_limit);
    }

    // Get charging state
    #[allow(clippy::match_same_arms)]
    let charging_summary = match charging_state {
        ChargingStateEnum::Starting => ChargingSummary::Charging,
        ChargingStateEnum::Charging => ChargingSummary::Charging,
        ChargingStateEnum::Complete => ChargingSummary::NotCharging,
        ChargingStateEnum::Stopped => ChargingSummary::NotCharging,
        ChargingStateEnum::Disconnected => ChargingSummary::Disconnected,
        ChargingStateEnum::NoPower => ChargingSummary::NotCharging,
    };

    // Start/stop charging as required.
    {
        use ShouldCharge::DoCharge;
        use ShouldCharge::DoNotCharge;
        #[allow(clippy::match_same_arms)]
        match charging_summary {
            ChargingSummary::Charging if should_charge == DoNotCharge => {
                info!(%id, "Stopping charge");
                sequence = sequence.set_should_charge(false);
            }
            ChargingSummary::Charging => {}
            ChargingSummary::NotCharging if should_charge == DoCharge && can_start_charge => {
                info!(%id, "Starting charge");
                sequence = sequence.set_should_charge(true);
            }
            ChargingSummary::NotCharging => {}
            ChargingSummary::Disconnected => info!(%id, "is disconnected"),
        }
    }

    // Send the commands.
    info!(%id, "Sending commands: {sequence:?}");
    tx.try_send(sequence);

    info!(%id, "All done.");
}

#[derive(Debug, Error)]
enum TeslamateError {
    #[error("Failed to enable logging: {0}")]
    Error(#[from] reqwest::Error),

    #[error("Failed to parse teslamate url: {0}")]
    ParseError(#[from] url::ParseError),
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
