pub mod command_processor;
pub mod monitor_charging;
pub mod monitor_doors;
pub mod monitor_location;
pub mod monitor_teslamate_location;
pub mod plug_in_reminder;
mod private;
pub mod token;

use crate::amber::car::ChargeRequest;

use monitor_doors::{DoorState, UserIsPresent};
use reqwest::Url;
use robotica_common::teslamate;
use robotica_tokio::services::tesla::api::{ChargingStateEnum, VehicleId};
use serde::{Deserialize, Serialize};

use robotica_common::mqtt::{Json, Parsed};
use robotica_tokio::pipes::stateful;

use super::InitState;

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct TeslamateId(u32);

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
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ShouldPlugin {
    ShouldPlugin,
    NoActionRequired,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ChargingInformation {
    battery_level: u8,
    charge_limit: u8,
    charge_request_at_home: ChargeRequest,
    // charge_request: ChargeRequest,
    charging_state: ChargingStateEnum,
}
