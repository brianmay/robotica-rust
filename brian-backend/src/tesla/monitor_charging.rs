use robotica_backend::{
    pipes::{stateful, stateless, Subscriber, Subscription},
    services::{persistent_state, tesla::api::ChargingStateEnum},
    spawn,
};
use robotica_common::{
    mqtt::{Json, MqttMessage, Parsed, QoS, Retain},
    robotica::{
        commands::Command,
        switch::{DeviceAction, DevicePower},
    },
};
use serde::{Deserialize, Serialize};
use tap::Pipe;
use thiserror::Error;
use tokio::select;
use tracing::{error, info};

use crate::{amber::car::ChargeRequest, InitState};

use super::{command_processor, ChargingInformation, Config, Receivers, TeslamateId};

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

pub struct Outputs {
    pub charging_information: stateful::Receiver<ChargingInformation>,
    pub commands: stateless::Receiver<command_processor::Command>,
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
    // last_success: DateTime<Utc>,
    // notified_errors: bool,
    // send_left_home_commands: bool,
}

impl TeslaState {
    const fn is_charging(&self) -> bool {
        self.charging_state.is_charging()
    }
}

#[allow(clippy::too_many_lines)]
pub fn monitor_charging(
    state: &InitState,
    config: &Config,
    receivers: Inputs,
) -> Result<Outputs, Error> {
    let id = config.teslamate_id.to_string();

    let (tx_summary, rx_summary) = stateful::create_pipe("tesla_charging_summary");
    let (tx_command, rx_command) = stateless::create_pipe("tesla_charging_command");

    let psr = state
        .persistent_state_database
        .for_name::<PersistentState>(&format!("tesla_{id}"));
    let ps = psr.load().unwrap_or_default();

    let mqtt = state.mqtt.clone();

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

    // let mut token = Token::get(&tesla_secret)?;

    let config = config.clone();
    spawn(async move {
        let name = &config.name;
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

        info!("{name}: Initial Tesla state: {:?}", tesla_state);

        tx_summary.try_send(ChargingInformation {
            battery_level: tesla_state.battery_level,
            charging_state: tesla_state.charging_state,
            charge_limit: tesla_state.charge_limit,
            charge_request_at_home: should_charge_at_home(&ps, amber_charge_request),
        });

        loop {
            let was_at_home = tesla_state.is_at_home;

            select! {
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

            update_auto_charge(ps.auto_charge, config.teslamate_id, &tesla_state, &mqtt);
        }
    });

    Outputs {
        charging_information: rx_summary,
        commands: rx_command,
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
    teslamate_id: TeslamateId,
    tesla_state: &TeslaState,
    mqtt: &robotica_backend::services::mqtt::MqttTx,
) {
    let notified_errors = false;
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

enum ChargingSummary {
    Charging,
    NotCharging,
    Disconnected,
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
fn check_charge(
    tesla: &Config,
    tx: &stateless::Sender<command_processor::Command>,
    tesla_state: &TeslaState,
    charge_request: ChargeRequest,
) {
    info!("Checking charge");
    let name = &tesla.name;

    let (should_charge, charge_limit) = should_charge(charge_request, tesla_state);

    // We should not attempt to start charging if charging is complete.
    let charging_state = tesla_state.charging_state;
    let can_start_charge = charging_state != ChargingStateEnum::Complete;

    info!("{name}: Current data: {charge_request:?}, {tesla_state:?}",);
    info!("{name}: Desired State: should charge: {should_charge:?}, can start charge: {can_start_charge}, charge limit: {charge_limit}");

    // Do we need to set the charge limit?
    let set_charge_limit =
        should_charge != ShouldCharge::DontTouch && tesla_state.charge_limit != charge_limit;

    // Construct sequence of commands to send to Tesla.
    let mut sequence = command_processor::Command::new();

    // Set the charge limit if required.
    if set_charge_limit {
        info!("{name}: Setting charge limit to {}", charge_limit);
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
                info!("{name}: Stopping charge");
                sequence = sequence.set_should_charge(false);
            }
            ChargingSummary::Charging => {}
            ChargingSummary::NotCharging if should_charge == DoCharge && can_start_charge => {
                info!("{name}: Starting charge");
                sequence = sequence.set_should_charge(true);
            }
            ChargingSummary::NotCharging => {}
            ChargingSummary::Disconnected => info!("{name}: is disconnected"),
        }
    }

    // Send the commands.
    info!("{name}: Sending commands: {sequence:?}");
    tx.try_send(sequence);

    info!("{name}: All done.");
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
