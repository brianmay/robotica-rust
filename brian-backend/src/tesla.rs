use crate::amber::{PriceQuality, PriceSummary};
use crate::delays::{delay_input, IsActive};

use anyhow::Result;
use log::debug;
use robotica_backend::services::tesla::api::{ChargeState, ChargingStateEnum, Token};
use robotica_common::robotica::DevicePower;
use std::fmt::Display;
use std::time::Duration;
use thiserror::Error;
use tokio::select;

use robotica_backend::entities::{create_stateless_entity, Receiver, StatefulData};
use robotica_backend::spawn;
use robotica_common::mqtt::MqttMessage;

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
    let rx2 = delay_input("tesla_doors (delayed)", duration, rx);

    spawn(async move {
        let mut s = rx2.subscribe().await;
        while let Ok((prev, open)) = s.recv().await {
            debug!("out received: {:?} {:?}", prev, open);
            if prev.is_none() {
                continue;
            }
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
    let pi_rx = state
        .subscriptions
        .subscribe_into_stateful::<bool>(&format!("teslamate/cars/{car_number}/plugged_in"));

    let auto_charge_rx = state
        .subscriptions
        .subscribe_into_stateful::<DevicePower>(&format!(
            "state/Tesla/{car_number}/AutoCharge/power"
        ));

    spawn(async move {
        let mut token = Token::get().unwrap();
        token.check().await.unwrap();
        let car_id = get_car_id(&mut token, car_number).await.unwrap().unwrap();

        let mut auto_charge_s = auto_charge_rx.subscribe().await;
        let mut rx_s = price_summary_rx.subscribe().await;
        let mut timer = tokio::time::interval(Duration::from_secs(5 * 60));
        let mut charge_state = token.get_charge_state(car_id).await.ok();
        let mut pi_s = pi_rx.subscribe().await;
        let mut price_summary: Option<PriceSummary> = None;

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
                Ok((_, new_price_summary)) = rx_s.recv() => {
                    log::info!("New price summary: {:?}", new_price_summary);
                    price_summary = Some(new_price_summary);
                }
                Ok((_, pi)) = pi_s.recv() => {
                    if pi {
                        log::info!("Car is plugged in");
                    } else {
                        log::info!("Car is disconnected");
                    }

                    match token.get_charge_state(car_id).await {
                        Ok(new_charge_state) => charge_state = Some(new_charge_state),
                        Err(err) => log::info!("Failed to get charge state: {err}"),
                    }
                }
                Ok((_, ac)) = auto_charge_s.recv() => {
                    if ac == DevicePower::On {
                        log::info!("Auto charge is on");
                    } else {
                        log::info!("Auto charge is off");
                    }
                }
                else => break,
            }

            if let Some(DevicePower::On) = auto_charge_rx.get_current().await {
                if let Some(price_summary) = &price_summary {
                    check_charge(car_id, &token, &mut charge_state, price_summary)
                        .await
                        .unwrap_or_else(|err| {
                            log::info!("Error checking charge: {}", err);
                        });
                }
            } else {
                log::info!("Skipping auto charge as off");
            }
        }
    });
}

async fn get_car_id(token: &mut Token, car_n: usize) -> Result<Option<u64>> {
    let vehicles = token.get_vehicles().await?;
    let vehicle = vehicles.get(car_n - 1);
    log::debug!("Got vehicle: {:?}", vehicle);
    let number = vehicle.map(|v| v.id);
    Ok(number)
}

async fn check_charge(
    car_id: u64,
    token: &Token,
    charge_state: &mut Option<ChargeState>,
    price_summary: &PriceSummary,
) -> Result<()> {
    let charging = charge_state
        .as_ref()
        .map_or_else(|| ChargingStateEnum::Stopped, |s| s.charging_state);

    // Should we turn on charging?
    let should_charge = price_summary.quality == PriceQuality::SuperCheap
        || price_summary.quality == PriceQuality::Cheap;

    // What is the limit we should charge to?
    let charge_limit = match price_summary.quality {
        PriceQuality::Expensive => 50,
        PriceQuality::Normal => 70,
        PriceQuality::Cheap => 80,
        PriceQuality::SuperCheap => 90,
    };

    // Is battery level low enough that we can charge it?
    let can_charge = match charge_state {
        Some(state) => state.battery_level < charge_limit,
        None => true,
    };

    log::debug!("Current data: {price_summary:?}, {charge_state:?}");
    log::debug!("Desired State: {should_charge}, {can_charge}, {charge_limit}");

    // Do we need to set the charge limit?
    let set_charge_limit = if let Some(charge_state) = charge_state {
        if charge_state.charge_limit_soc != charge_limit {
            log::debug!("Charge limit is wrong, setting it");
            true
        } else {
            false
        }
    } else {
        log::debug!("Charge limit is unknown, setting it");
        true
    };

    // Set the charge limit if required.
    if set_charge_limit {
        log::info!("Setting charge limit to {}", charge_limit);
        token.wait_for_wake_up(car_id).await?;
        token
            .set_charge_limit(car_id, charge_limit)
            .await
            .unwrap_or_else(|err| {
                log::error!("Failed to set charge limit: {}", err);
            });
    }

    // Start/stop charging as required.
    let get_charge_state = if charging == ChargingStateEnum::Charging && !should_charge {
        log::info!("Stopping charge");
        token.wait_for_wake_up(car_id).await?;
        token.charge_stop(car_id).await.unwrap_or_else(|err| {
            log::info!("Failed to stop charge: {err}");
        });
        true
    } else if charging == ChargingStateEnum::Stopped && should_charge && can_charge {
        log::info!("Starting charge");
        token.wait_for_wake_up(car_id).await?;
        token.charge_start(car_id).await.unwrap_or_else(|err| {
            log::info!("Failed to start charge: {err}");
        });
        true
    } else if charging == ChargingStateEnum::Complete && should_charge && can_charge {
        log::info!("Restarting charge");
        token.wait_for_wake_up(car_id).await?;
        token.charge_start(car_id).await.unwrap_or_else(|err| {
            log::info!("Failed to start charge: {err}");
        });
        true
    } else {
        false
    };

    // Get the charge state again, vehicle should be awake now.
    if set_charge_limit || get_charge_state {
        log::info!("Getting charge state (2)");
        *charge_state = Some(token.get_charge_state(car_id).await?);
    }

    log::debug!("All done.");
    Ok(())
}
