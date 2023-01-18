//! Wrapper around Tesla's API

use std::time::Duration;

use reqwest::Error;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use tokio::time::{sleep, Instant};

use crate::{
    is_debug_mode,
    services::persistent_state::{self, PersistentStateRow},
};

async fn post<T: Serialize + Sync, U: DeserializeOwned>(url: &str, body: &T) -> Result<U, Error> {
    log::debug!("post {}", url);

    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(body)
        .timeout(Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?;

    let text = response.json().await?;
    log::debug!("post done {}", url);
    Ok(text)
}

// async fn get<U: DeserializeOwned>(url: &str) -> Result<U, Error> {
//     let client = reqwest::Client::new();
//     let response = client
//         .get(url)
//         .header("Content-Type", "application/json")
//         .timeout(Duration::from_secs(30))
//         .send()
//         .await?
//         .error_for_status()?;

//     let text = response.json().await?;
//     Ok(text)
// }

async fn get_with_token<U: DeserializeOwned>(url: &str, token: &str) -> Result<U, Error> {
    log::debug!("get_with_token: {}", url);

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .timeout(Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?;

    // let text = response.text().await?;
    // println!("{}", text);
    // let text = serde_json::from_str(&text).unwrap();

    let text = response.json().await?;
    log::debug!("get_with_token done: {}", url);
    Ok(text)
}

async fn post_with_token<T: Serialize + Sync, U: DeserializeOwned>(
    url: &str,
    token: &str,
    body: &T,
) -> Result<U, Error> {
    log::debug!("post_with_token: {}", url);

    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .json(body)
        .timeout(Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?;

    // let text = response.text().await?;
    // println!("{}", text);
    // let text = serde_json::from_str(&text).unwrap();

    let text = response.json().await?;
    log::debug!("post_with_token done: {}", url);
    Ok(text)
}

#[derive(Serialize)]
struct TokenRenew {
    grant_type: String,
    client_id: String,
    refresh_token: String,
    scope: String,
}

/// Token to access the Tesla API
#[derive(Serialize, Deserialize)]
pub struct Token {
    access_token: String,
    refresh_token: String,
    id_token: String,

    expires_in: Option<u64>,
    token_type: Option<String>,

    /// Time can be renewed.
    #[serde(skip)]
    pub renew_at: Option<Instant>,

    /// Time when the token expires.
    #[serde(skip)]
    pub expires_at: Option<Instant>,
}

#[derive(Serialize)]
struct SetChargeLimit {
    percent: u8,
}

/// Vehicle information
#[derive(Deserialize, Debug)]
pub struct Vehicle {
    /// Vehicle ID for owner-api endpoint.
    pub id: u64,
    /// Vehicle ID for streaming or Auto park API.
    pub vehicle_id: u64,

    /// Vehicle identification number.
    pub vin: String,

    /// Vehicle display name.
    pub display_name: String,
    // option_codes: String,
    // color: Option<String>,
    // tokens: Vec<String>,
    // state: String,
    // in_service: bool,
    // id_s: String,
    // calendar_enabled: bool,
    // api_version: u8,
    // backseat_token: Option<String>,
    // backseat_token_updated_at: Option<String>,
}

/// Is the car currently charging?
#[derive(Debug, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum ChargingStateEnum {
    /// Charging is starting
    Starting,

    /// Charging is complete
    Complete,

    /// Charging is in progress
    Charging,

    /// Charging is not in progress and we are disconnected
    Disconnected,

    /// Charging is not in progress
    Stopped,

    /// Charger cable is connected but not getting power
    NoPower,
}

impl ChargingStateEnum {
    /// Is the car currently charging?
    #[must_use]
    pub const fn is_charging(self) -> bool {
        match self {
            ChargingStateEnum::Starting | ChargingStateEnum::Charging => true,
            ChargingStateEnum::Complete
            | ChargingStateEnum::Disconnected
            | ChargingStateEnum::Stopped
            | ChargingStateEnum::NoPower => false,
        }
    }
}

/// Charging state
#[derive(Debug, Deserialize)]
pub struct ChargeState {
    /// Battery level in percent
    pub battery_level: u8,

    /// The current charge limit in percent
    pub charge_limit_soc: u8,

    /// The current charge state
    pub charging_state: ChargingStateEnum,

    /// Thee estimated time to charge in seconds
    pub time_to_full_charge: f32,
}

/// The response from a generic request
#[derive(Deserialize)]
pub struct GenericResponse {
    /// The reason for an error or ""
    reason: String,

    /// The result of the request
    result: bool,
}

/// The response from a wake up request
#[derive(Deserialize)]
pub struct WakeUpResponse {
    state: String,
}

#[derive(Debug, Deserialize)]
struct OuterResponse<T> {
    pub response: T,
}

type OuterVehiclesResponse = OuterResponse<Vec<Vehicle>>;
type OuterChargeState = OuterResponse<ChargeState>;
type OuterWakeUpResponse = OuterResponse<WakeUpResponse>;
type OuterGenericResponse = OuterResponse<GenericResponse>;

/// Error when something went wrong with the Token
#[derive(Debug, Error)]
pub enum TokenError {
    /// A error loading/saving the persistent token
    #[error("Persistent state error: {0}")]
    Error(#[from] persistent_state::Error),

    /// Reqwest error
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
}

/// A generic error return from the API
#[derive(Debug, Error)]
pub enum GenericError {
    /// The HTTP request failed.
    #[error("Tesla reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// The HTTP request succeeded, but the response was not successful.
    #[error("Generic tesla error: {0}")]
    Failed(String),
}

impl From<OuterGenericResponse> for Result<(), GenericError> {
    fn from(response: OuterGenericResponse) -> Self {
        if response.response.result {
            Ok(())
        } else if response.response.reason.is_empty() {
            Err(GenericError::Failed("no reason".into()))
        } else {
            Err(GenericError::Failed(response.response.reason))
        }
    }
}

/// An error occurred while trying to wake up the car
#[derive(Debug, Error)]
pub enum WakeupError {
    /// The HTTP request failed.
    #[error("Wakeup reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// We couldn't wake up the car before the timeout elapsed
    #[error("Wakeup timeout error")]
    Timeout,
}

/// An error occurred while running a sequence of commands
#[derive(Debug, Error)]
pub enum SequenceError {
    /// The HTTP request failed.
    #[error("Tesla reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// The HTTP request succeeded, but the response was not successful.
    #[error("Generic tesla error: {0}")]
    Failed(String),

    /// We couldn't wake up the car before the timeout elapsed
    #[error("Wakeup timeout error")]
    Timeout,
}

impl From<WakeupError> for SequenceError {
    fn from(error: WakeupError) -> Self {
        match error {
            WakeupError::Reqwest(e) => SequenceError::Reqwest(e),
            WakeupError::Timeout => SequenceError::Timeout,
        }
    }
}

impl From<GenericError> for SequenceError {
    fn from(error: GenericError) -> Self {
        match error {
            GenericError::Reqwest(e) => SequenceError::Reqwest(e),
            GenericError::Failed(e) => SequenceError::Failed(e),
        }
    }
}

impl Token {
    /// Load token from file
    ///
    /// # Errors
    ///
    /// Returns `TokenError::Environment` if the environment variable `TESLA_SECRET_FILE` is not set.
    /// Returns `TokenError::Io` if the file could not be read.
    /// Returns `TokenError::Json` if the file could not be deserialized.
    /// Returns `TokenError::Reqwest` if the token could not be refreshed.
    pub fn get(ps: &PersistentStateRow<Token>) -> Result<Self, persistent_state::Error> {
        let token = ps.load()?;
        Ok(token)
    }

    fn put(&self, ps: &PersistentStateRow<Token>) -> Result<(), persistent_state::Error> {
        ps.save(self)?;
        Ok(())
    }

    async fn renew(&self) -> Result<Self, Error> {
        let url = "https://auth.tesla.com/oauth2/v3/token";
        let body = TokenRenew {
            grant_type: "refresh_token".into(),
            client_id: "ownerapi".into(),
            refresh_token: self.refresh_token.clone(),
            scope: "openid email offline_access".into(),
        };

        let mut token: Token = post(url, &body).await?;

        if let Some(expires_in) = token.expires_in {
            let expires_in = Duration::from_secs(expires_in);
            let renew_in = expires_in
                .checked_sub(Duration::from_secs(60 * 60))
                .unwrap_or_default();
            token.renew_at = Some(Instant::now() + renew_in);
            token.expires_at = Some(Instant::now() + expires_in);
        }

        Ok(token)
    }

    /// Renew the token if it is going to expire soon
    ///
    /// # Errors
    ///
    /// Returns `TokenError::Reqwest` if the HTTP request failed.
    /// Returns `TokenError::Json` if the response could not be deserialized.
    /// Returns `TokenError::Io` if the token could not be written to disk.
    /// Returns `TokenError::Environment` if the environment variable `TESLA_SECRET_FILE` is not set.
    pub async fn check(&mut self, ps: &PersistentStateRow<Token>) -> Result<(), TokenError> {
        if let Some(renew_at) = self.expires_at {
            if renew_at > Instant::now() {
                return Ok(());
            }
        }

        let token = self.renew().await?;
        token.put(ps)?;
        *self = token;
        Ok(())
    }

    /// Try to wake up the car
    ///
    /// # Errors
    ///
    /// Returns `WakeupError::Reqwest` if the HTTP request failed.
    pub async fn wake_up(&self, id: u64) -> Result<WakeUpResponse, Error> {
        let url = format!("https://owner-api.teslamotors.com/api/1/vehicles/{id}/wake_up");
        let response: OuterWakeUpResponse = post_with_token(&url, &self.access_token, &()).await?;
        Ok(response.response)
    }

    /// Get all the cars for the current token
    ///
    /// # Errors
    ///
    /// Returns error if the HTTP request failed.
    pub async fn get_vehicles(&self) -> Result<Vec<Vehicle>, Error> {
        let url = "https://owner-api.teslamotors.com/api/1/vehicles";
        let response: OuterVehiclesResponse = get_with_token(url, &self.access_token).await?;
        Ok(response.response)
    }

    /// Wait for the car to wake up
    ///
    /// # Errors
    ///
    /// Returns `WakeupError::Reqwest` if the HTTP request failed.
    /// Returns `WakeupError::Timeout` if the car didn't wake up before the timeout elapsed.
    pub async fn wait_for_wake_up(&self, id: u64) -> Result<(), WakeupError> {
        let timeout = Instant::now() + Duration::from_secs(60);

        log::info!("Trying to wake up (initial)");
        let response = self.wake_up(id).await?;
        if response.state == "online" {
            log::info!("Car is already online");
            return Ok(());
        }

        while Instant::now() < timeout {
            log::info!("Trying to wake up (retry)");
            let response = self.wake_up(id).await?;
            if response.state == "online" {
                log::info!("Car is online");
                sleep(Duration::from_secs(30)).await;
                log::info!("Car is online (after sleep)");
                return Ok(());
            }
            log::info!("Car is not online");
            sleep(Duration::from_secs(5)).await;
        }

        Err(WakeupError::Timeout)
    }

    /// Request the car start charging
    ///
    /// # Errors
    ///
    /// Returns `GenericError::Reqwest` if the HTTP request failed.
    /// Returns `GenericError::Failed` if the request was not successful.
    pub async fn charge_start(&self, id: u64) -> Result<(), GenericError> {
        let url =
            format!("https://owner-api.teslamotors.com/api/1/vehicles/{id}/command/charge_start");
        let response: OuterGenericResponse = post_with_token(&url, &self.access_token, &()).await?;
        response.into()
    }

    /// Request the car stop charging
    ///
    /// # Errors
    ///
    /// Returns `GenericError::Reqwest` if the HTTP request failed.
    /// Returns `GenericError::Failed` if the request was not successful.
    pub async fn charge_stop(&self, id: u64) -> Result<(), GenericError> {
        let url =
            format!("https://owner-api.teslamotors.com/api/1/vehicles/{id}/command/charge_stop");
        let response: OuterGenericResponse = post_with_token(&url, &self.access_token, &()).await?;
        response.into()
    }

    /// Set the charge limit for the car
    ///
    /// # Errors
    ///
    /// Returns `GenericError::Reqwest` if the HTTP request failed.
    /// Returns `GenericError::Failed` if the request was not successful.
    pub async fn set_charge_limit(&self, id: u64, percent: u8) -> Result<(), GenericError> {
        let url = format!(
            "https://owner-api.teslamotors.com/api/1/vehicles/{id}/command/set_charge_limit"
        );
        let body = SetChargeLimit { percent };
        let response: OuterGenericResponse =
            post_with_token(&url, &self.access_token, &body).await?;

        if !response.response.result && response.response.reason == "already_set" {
            return Ok(());
        }

        response.into()
    }

    /// Get the charge state for the car
    ///
    /// # Errors
    ///
    /// Returns `GenericError::Reqwest` if the HTTP request failed.
    /// Returns `GenericError::Failed` if the request was not successful.
    pub async fn get_charge_state(&self, id: u64) -> Result<ChargeState, Error> {
        let url = format!(
            "https://owner-api.teslamotors.com/api/1/vehicles/{id}/data_request/charge_state"
        );
        let response: OuterChargeState = get_with_token(&url, &self.access_token).await?;
        Ok(response.response)
    }
}

#[derive(Debug)]
enum Command {
    WakeUp,
    SetChargeLimit(u8),
    ChargeStart,
    ChargeStop,
}

impl Command {
    async fn execute(&self, token: &Token, id: u64) -> Result<(), SequenceError> {
        match self {
            Command::WakeUp => token.wait_for_wake_up(id).await?,
            Command::SetChargeLimit(percent) => token.set_charge_limit(id, *percent).await?,
            Command::ChargeStart => token.charge_start(id).await?,
            Command::ChargeStop => token.charge_stop(id).await?,
        }
        Ok(())
    }
}

#[derive(Debug)]
/// A sequence of commands to execute
pub struct CommandSequence {
    /// prefix commands are not executed unless there is at least one real command.
    prefix_commands: Vec<Command>,
    /// The commands to execute
    commands: Vec<Command>,
}

impl CommandSequence {
    /// Create a new command sequence
    #[must_use]
    pub const fn new() -> Self {
        CommandSequence {
            prefix_commands: vec![],
            commands: vec![],
        }
    }

    /// Add a command to the sequence
    fn add(&mut self, command: Command) {
        self.commands.push(command);
    }

    /// Wake up the car
    pub fn add_wake_up(&mut self) {
        self.prefix_commands.push(Command::WakeUp);
    }

    /// Set the charge limit for the car
    pub fn add_set_chart_limit(&mut self, percent: u8) {
        self.add(Command::SetChargeLimit(percent));
    }

    /// Request the car start charging
    pub fn add_charge_start(&mut self) {
        self.add(Command::ChargeStart);
    }

    /// Request the car stop charging
    pub fn add_charge_stop(&mut self) {
        self.add(Command::ChargeStop);
    }

    /// Execute the sequence
    ///
    /// # Errors
    ///
    /// Returns error if the wake up request failed.
    /// Returns error if any of the commands failed.
    pub async fn execute(&self, token: &Token, car_id: u64) -> Result<(), SequenceError> {
        if self.commands.is_empty() {
            return Ok(());
        }

        if is_debug_mode() {
            log::debug!("Would execute commands: {:?}", self.commands);
            return Ok(());
        }

        for command in &self.prefix_commands {
            command.execute(token, car_id).await?;
        }

        for command in &self.commands {
            command.execute(token, car_id).await?;
        }

        Ok(())
    }

    /// Is the sequence empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl Default for CommandSequence {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::similar_names)]
    use crate::services::persistent_state::PersistentStateDatabase;

    use super::*;

    #[ignore = "requires secrets"]
    #[tokio::test]
    async fn test_get_token() {
        let psd = PersistentStateDatabase::new().unwrap();
        let psr = psd.for_name("tesla_token").unwrap();

        let token = Token::get(&psr).unwrap();

        let token = token.renew().await.unwrap();
        // token.wait_for_wake_up(&id.to_string()).await.unwrap();
        // token.charge_start(id).await.unwrap();
        // token.charge_stop(id).await.unwrap();
        // token.set_charge_limit(id, 88).await.unwrap();
        let vehicles = token.get_vehicles().await.unwrap();
        println!("{vehicles:#?}");

        // let charge_state = token.get_charge_state(id).await.unwrap();
        // println!("{charge_state:#?}");

        token.put(&psr).unwrap();
    }
}
