use crate::amber::{PriceCategory, PriceSummary};
use crate::delays::{delay_input, delay_repeat, IsActive};

use anyhow::Result;
use log::debug;
use robotica_backend::services::tesla::api::{
    ChargeState, ChargingStateEnum, CommandSequence, Token,
};
use robotica_common::robotica::{Command, DeviceAction, DevicePower};
use std::fmt::Display;
use std::time::Duration;
use thiserror::Error;
use tokio::select;

use robotica_backend::entities::{create_stateless_entity, Receiver, StatefulData};
use robotica_backend::spawn;
use robotica_common::mqtt::{MqttMessage, QoS};

use super::State;

#[derive(Clone, Debug, Eq, PartialEq)]
enum TeslaDoorState {
    Open,
    Closed,
}

impl Display for TeslaDoorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeslaDoorState::Open => write!(f, "open"),
            TeslaDoorState::Closed => write!(f, "closed"),
        }
    }
}

impl TryFrom<MqttMessage> for TeslaDoorState {
    type Error = TeslaStateErr;
    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        match msg.payload.as_str() {
            "true" => Ok(TeslaDoorState::Open),
            "false" => Ok(TeslaDoorState::Closed),
            _ => Err(TeslaStateErr::InvalidDoorState(msg.payload)),
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
            TeslaUserIsPresent::UserPresent => write!(f, "user is present"),
            TeslaUserIsPresent::UserNotPresent => write!(f, "user is not present"),
        }
    }
}

impl TryFrom<MqttMessage> for TeslaUserIsPresent {
    type Error = TeslaStateErr;
    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        match msg.payload.as_str() {
            "true" => Ok(TeslaUserIsPresent::UserPresent),
            "false" => Ok(TeslaUserIsPresent::UserNotPresent),
            _ => Err(TeslaStateErr::InvalidDoorState(msg.payload)),
        }
    }
}

#[derive(Error, Debug)]
pub enum TeslaStateErr {
    #[error("Invalid door state: {0}")]
    InvalidDoorState(String),

    #[error("Invalid UTF8")]
    Utf8Error(#[from] std::str::Utf8Error),
}

impl IsActive for Vec<&str> {
    fn is_active(&self) -> bool {
        !self.is_empty()
    }
}

pub fn monitor_tesla_doors(state: &mut State, car_number: usize) {
    let fo_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaDoorState>(&format!(
            "teslamate/cars/{car_number}/frunk_open"
        ));
    let to_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaDoorState>(&format!(
            "teslamate/cars/{car_number}/trunk_open"
        ));
    let do_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaDoorState>(&format!(
            "teslamate/cars/{car_number}/doors_open"
        ));
    let wo_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaDoorState>(&format!(
            "teslamate/cars/{car_number}/windows_open"
        ));
    let up_rx = state
        .subscriptions
        .subscribe_into_stateful::<TeslaUserIsPresent>(&format!(
            "teslamate/cars/{car_number}/is_user_present"
        ));

    let message_sink = state.message_sink.clone();

    let (tx, rx) = create_stateless_entity("tesla_doors");

    spawn(async move {
        let mut fo_s = fo_rx.subscribe().await;
        let mut to_s = to_rx.subscribe().await;
        let mut do_s = do_rx.subscribe().await;
        let mut wo_s = wo_rx.subscribe().await;
        let mut up_s = up_rx.subscribe().await;

        loop {
            select! {
                Ok((_, _)) = fo_s.recv() => {},
                Ok((_, _)) = to_s.recv() => {},
                Ok((_, _)) = do_s.recv() => {},
                Ok((_, _)) = wo_s.recv() => {},
                Ok((_, _)) = up_s.recv() => {},
                else => break,
            };

            let mut open: Vec<&str> = vec![];

            let maybe_up = up_rx.get_current().await;
            if let Some(TeslaUserIsPresent::UserNotPresent) = maybe_up {
                let maybe_fo = fo_rx.get_current().await;
                let maybe_to = to_rx.get_current().await;
                let maybe_do = do_rx.get_current().await;
                let maybe_wo = wo_rx.get_current().await;

                debug!(
                    "fo: {:?}, to: {:?}, do: {:?}, wo: {:?}, up: {:?}",
                    maybe_fo, maybe_to, maybe_do, maybe_wo, maybe_up
                );

                if let Some(TeslaDoorState::Open) = maybe_fo {
                    open.push("frunk")
                }

                if let Some(TeslaDoorState::Open) = maybe_to {
                    open.push("boot")
                }

                if let Some(TeslaDoorState::Open) = maybe_do {
                    open.push("doors")
                }

                if let Some(TeslaDoorState::Open) = maybe_wo {
                    open.push("windows")
                }
            } else {
                debug!("up: {:?}", maybe_up);
            }

            debug!("open: {:?}", open);
            tx.try_send(open);
        }
    });

    let duration = Duration::from_secs(60);

    // We only care if doors open for at least duration.
    let rx = delay_input("tesla_doors (delayed)", duration, rx);

    // Discard initial [] value and duplicate events.
    let rx = rx
        .map_into_stateful(|f| f)
        .filter_into_stateless(|(p, c)| p.is_some() || c.is_active())
        .map_into_stateless(|(_, c)| c);

    // Repeat the last value duration time.
    let rx = delay_repeat("tesla_doors (repeat)", duration, rx);

    // Output the message.
    spawn(async move {
        let mut s = rx.subscribe().await;
        while let Ok(open) = s.recv().await {
            debug!("open received: {:?}", open);
            let msg = if open.is_empty() {
                "The Tesla is secure".to_string()
            } else if open.len() == 1 {
                format!("The Tesla {} is open", open.join(", "))
            } else {
                format!("The Tesla {} are open", open.join(", "))
            };
            message_sink.try_send(msg);
        }
    });
}

pub fn monitor_charging(
    state: &mut State,
    car_number: usize,
    price_summary_rx: Receiver<StatefulData<PriceSummary>>,
) {
    let mqtt = state.mqtt.clone();

    let price_category_rx = price_summary_rx.map_into_stateful(|(_, ps)| ps.category);

    let pi_rx = state
        .subscriptions
        .subscribe_into_stateful::<bool>(&format!("teslamate/cars/{car_number}/plugged_in"));

    let auto_charge_rx = {
        let mqtt = mqtt.clone();
        state
            .subscriptions
            .subscribe_into_stateless::<Command>(&format!("command/Tesla/{car_number}/AutoCharge"))
            .map_into_stateless(move |cmd| {
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
            .subscribe_into_stateless::<Command>(&format!("command/Tesla/{car_number}/ForceCharge"))
            .map_into_stateless(move |cmd| {
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
            .subscribe_into_stateless::<String>(&format!("state/Tesla/{car_number}/Location"))
            .map_into_stateful(move |location| location == "home")
    };

    spawn(async move {
        let mut token = Token::get().unwrap();
        token.check().await.unwrap();
        let car_id = get_car_id(&mut token, car_number).await.unwrap().unwrap();

        let mut auto_charge_s = auto_charge_rx.subscribe().await;
        let mut force_charge_s = force_charge_rx.subscribe().await;
        let mut location_charge_s = is_home_rx.subscribe().await;
        let mut price_quality_rx = price_category_rx.subscribe().await;
        let mut timer = tokio::time::interval(Duration::from_secs(5 * 60));
        let mut charge_state = None;
        let mut pi_s = pi_rx.subscribe().await;
        let mut price_category: Option<PriceCategory> = None;

        let mut auto_charge = false;
        let mut force_charge = false;
        let mut is_home = false;

        log::info!("Initial charge state: {charge_state:?}");

        loop {
            select! {
                _ = timer.tick() => {
                    log::info!("Refreshing state, token expiration: {:?}", token.expires_at);
                    token.check().await.unwrap_or_else(|e| {
                        log::error!("Error refreshing token: {}", e);
                    });
                    log::info!("Token expiration: {:?}", token.expires_at);
                }
                Ok((_, new_price_category)) = price_quality_rx.recv() => {
                    log::info!("New price summary: {:?}", new_price_category);
                    price_category = Some(new_price_category);
                }
                Ok((_, pi)) = pi_s.recv() => {
                    if pi {
                        log::info!("Car is plugged in");
                    } else {
                        log::info!("Car is disconnected");
                    }
                    charge_state = None;
                }
                Ok(cmd) = auto_charge_s.recv() => {
                    if let Command::Device(cmd) = cmd {
                        auto_charge = match cmd.action {
                            DeviceAction::TurnOn => true,
                            DeviceAction::TurnOff => false,
                        };
                        log::info!("Auto charge: {auto_charge}");
                        update_auto_charge(auto_charge, car_number, &charge_state, &mqtt);
                    } else {
                        log::info!("Ignoring invalid auto_charge command: {cmd:?}");
                    }
                }
                Ok(cmd) = force_charge_s.recv() => {
                    if let Command::Device(cmd) = cmd {
                        force_charge = match cmd.action {
                            DeviceAction::TurnOn => true,
                            DeviceAction::TurnOff => false,
                        };
                        log::info!("Force charge: {force_charge}");
                        update_force_charge(&charge_state, car_number, force_charge, &mqtt);
                    } else {
                        log::info!("Ignoring invalid force_charge command: {cmd:?}");
                    }
                }
                Ok((_, new_is_home)) = location_charge_s.recv() => {
                    is_home = new_is_home;
                    log::info!("Location is home: {is_home}");
                    charge_state = None;
                }
                else => break,
            }

            if is_home && auto_charge {
                if let Some(price_category) = &price_category {
                    check_charge(
                        car_id,
                        &token,
                        &mut charge_state,
                        price_category,
                        force_charge,
                    )
                    .await;
                } else {
                    log::info!("No price summary available, skipping charge check");
                }
            } else {
                log::info!("Skipping charge check");
                charge_state = None;
            }

            update_auto_charge(auto_charge, car_number, &charge_state, &mqtt);
            update_force_charge(&charge_state, car_number, force_charge, &mqtt);
        }
    });
}

fn update_auto_charge(
    auto_charge: bool,
    car_number: usize,
    charge_state: &Option<ChargeState>,
    mqtt: &robotica_backend::services::mqtt::Mqtt,
) {
    let is_charging = charge_state.as_ref().map_or_else(
        || false,
        |s| s.charging_state == ChargingStateEnum::Charging,
    );

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
    mqtt: &robotica_backend::services::mqtt::Mqtt,
) {
    let topic = format!("state/Tesla/{car_number}/AutoCharge/power");
    let string: String = status.into();
    let msg = MqttMessage::new(&topic, string, true, QoS::AtLeastOnce);
    mqtt.try_send(msg);
}

fn update_force_charge(
    charge_state: &Option<ChargeState>,
    car_number: usize,
    force_charge: bool,
    mqtt: &robotica_backend::services::mqtt::Mqtt,
) {
    let is_charging = charge_state.as_ref().map_or_else(
        || false,
        |s| s.charging_state == ChargingStateEnum::Charging,
    );
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
    mqtt: &robotica_backend::services::mqtt::Mqtt,
) {
    let topic = format!("state/Tesla/{car_number}/ForceCharge/power");
    let string: String = status.into();
    let msg = MqttMessage::new(&topic, string, true, QoS::AtLeastOnce);
    mqtt.try_send(msg);
}

async fn get_car_id(token: &mut Token, car_n: usize) -> Result<Option<u64>> {
    let vehicles = token.get_vehicles().await?;
    let vehicle = vehicles.get(car_n - 1);
    log::debug!("Got vehicle: {:?}", vehicle);
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
    fn min_charge(self, min_charge: u8) -> Self {
        match self {
            RequestedCharge::DontCharge => RequestedCharge::ChargeTo(min_charge),
            RequestedCharge::ChargeTo(limit) if limit < min_charge => {
                RequestedCharge::ChargeTo(min_charge)
            }
            _ => self,
        }
    }
}

async fn check_charge(
    car_id: u64,
    token: &Token,
    charge_state: &mut Option<ChargeState>,
    price_category: &PriceCategory,
    force_charge: bool,
) {
    // true state means car is awake; false means not sure.
    let car_is_awake = if charge_state.is_none() {
        log::info!("Waking up car");
        match token.wait_for_wake_up(car_id).await {
            Ok(_) => {
                log::info!("Car is awake; getting charge state");
                *charge_state = get_charge_state(token, car_id).await;
                true
            }
            Err(err) => {
                log::error!("Error waking up car: {err}");
                false
            }
        }
    } else {
        false
    };

    // What is the limit we should charge to?
    let requested_charge = match &price_category {
        PriceCategory::Expensive => RequestedCharge::DontCharge,
        PriceCategory::Normal => RequestedCharge::DontCharge,
        PriceCategory::Cheap => RequestedCharge::ChargeTo(80),
        PriceCategory::SuperCheap => RequestedCharge::ChargeTo(90),
    };

    // If we're forcing a charge, we should charge to at least 70%.
    let requested_charge = if force_charge {
        requested_charge.min_charge(70)
    } else {
        requested_charge
    };

    // Should we charge? If so, to what limit?
    let (should_charge, charge_limit) = match requested_charge {
        RequestedCharge::DontCharge => (false, 50),
        RequestedCharge::ChargeTo(limit) => (true, limit),
    };

    // Is battery level low enough that we can charge it?
    let can_charge = match charge_state {
        Some(state) => state.battery_level < charge_limit,
        None => true,
    };

    log::info!("Current data: {price_category:?}, {charge_state:?}, force charge: {force_charge}");
    log::info!("Desired State: should charge: {should_charge}, can charge: {can_charge}, charge limit: {charge_limit}");

    // Do we need to set the charge limit?
    let set_charge_limit = if let Some(charge_state) = charge_state {
        charge_state.charge_limit_soc != charge_limit
    } else {
        true
    };

    // Construct sequence of commands to send to Tesla.
    let mut sequence = CommandSequence::new();

    // Wake up the car if it's not already awake.
    if !car_is_awake {
        sequence.add_wake_up();
    }

    // Set the charge limit if required.
    if set_charge_limit {
        log::info!("Setting charge limit to {}", charge_limit);
        sequence.add_set_chart_limit(charge_limit);
    }

    // Get charging state
    let charging = charge_state.as_ref().map(|s| s.charging_state);
    let charging_summary = match charging {
        Some(ChargingStateEnum::Starting) => ChargingSummary::Charging,
        Some(ChargingStateEnum::Charging) => ChargingSummary::Charging,
        Some(ChargingStateEnum::Complete) => ChargingSummary::NotCharging,
        Some(ChargingStateEnum::Stopped) => ChargingSummary::NotCharging,
        Some(ChargingStateEnum::Disconnected) => ChargingSummary::Disconnected,
        None => ChargingSummary::Unknown,
    };

    // Start/stop charging as required.
    match charging_summary {
        ChargingSummary::Charging if !should_charge => {
            log::info!("Stopping charge");
            sequence.add_charge_stop();
        }
        ChargingSummary::Charging => {}
        ChargingSummary::NotCharging if should_charge && can_charge => {
            log::info!("Starting charge");
            sequence.add_charge_start();
        }
        ChargingSummary::NotCharging => {}
        ChargingSummary::Unknown if !should_charge => {
            log::info!("Stopping charge (unknown)");
            sequence.add_charge_stop();
        }
        ChargingSummary::Unknown if should_charge && can_charge => {
            log::info!("Starting charge (unknown)");
            sequence.add_charge_start();
        }
        ChargingSummary::Unknown => {}
        ChargingSummary::Disconnected => log::info!("Car is disconnected"),
    }

    // Send the commands.
    log::info!("Sending commands: {sequence:?}");
    sequence.execute(token, car_id).await.unwrap_or_else(|err| {
        log::info!("Error executing command sequence: {}", err);
    });

    // Get the charge state again, vehicle should be awake now.
    //
    // We do this even if the execute failed, because the execute may have
    // changed the cars state regardless.
    if !sequence.is_empty() || charge_state.is_none() {
        *charge_state = get_charge_state(token, car_id).await;
    }

    log::info!("All done.");
}

async fn get_charge_state(token: &Token, car_id: u64) -> Option<ChargeState> {
    log::info!("Getting charge state");
    let charge_state = token
        .get_charge_state(car_id)
        .await
        .map_err(|err| {
            log::info!("Failed to get charge state: {err}");
            err
        })
        .ok();
    log::info!("Got charge state: {charge_state:?}");
    charge_state
}
