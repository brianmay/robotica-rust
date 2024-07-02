//! Wrapper around Tesla's API

use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use opentelemetry::{global, metrics::Counter, KeyValue};
use robotica_common::{datetime::duration, mqtt::MqttMessage, unsafe_time_delta};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tap::Pipe;
use thiserror::Error;
use tracing::{debug, error, info};

use crate::{
    is_debug_mode,
    services::persistent_state::{self, PersistentStateRow},
};

/// A set of meter counters for the Tesla API
#[derive(Debug)]
pub struct Meters {
    auth_requests: Counter<u64>,
    vehicle_requests: Counter<u64>,
    other_requests: Counter<u64>,
}

impl Meters {
    /// Create a new set of meter counters
    #[must_use]
    pub fn new() -> Self {
        let attributes = vec![];
        let meter = global::meter_with_version(
            "tesla::api",
            None::<String>,
            None::<String>,
            Some(attributes),
        );

        Meters {
            auth_requests: meter.u64_counter("auth_requests").init(),
            vehicle_requests: meter.u64_counter("vehicle_requests").init(),
            other_requests: meter.u64_counter("other_requests").init(),
        }
    }
}

impl Default for Meters {
    fn default() -> Self {
        Self::new()
    }
}

/// A vehicle ID for the owner-api endpoint.
#[derive(Copy, Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct VehicleId(u64);

impl ToString for VehicleId {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

/// Error when something went wrong with the API
#[derive(Debug, Error)]

pub enum Error {
    /// Reqwest error
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Json error
    #[error("Json error: {0}")]
    Json(#[from] serde_json::Error),

    /// rate limit error
    #[error("Rate limit error, retry in: {}", duration::to_string(.0))]
    RateLimit(Duration),
}

#[tracing::instrument]
fn handle_error(
    response: Result<reqwest::Response, reqwest::Error>,
) -> Result<reqwest::Response, Error> {
    match response {
        Ok(response) => {
            if response.status() == 429 {
                let headers = response.headers();
                let retry_time = headers
                    .get("Retry-After")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(60)
                    .pipe(Duration::from_secs);

                info!(
                    "Got 429 rate limited, retry in: {}",
                    duration::to_string(&retry_time)
                );

                for (name, value) in headers {
                    info!("rate limit header header {}: {:?}", name, value);
                }

                return Err(Error::RateLimit(retry_time));
            }
            response.error_for_status()?.pipe(Ok)
        }
        Err(e) => {
            error!("Reqwest error: {}", e);
            Err(Error::Reqwest(e))
        }
    }
}

#[derive(Debug)]
enum AuthOperation {
    RenewToken,
}

#[tracing::instrument]
fn increment_auth_count(
    url: &str,
    operation: AuthOperation,
    result: Result<RawToken, Error>,
    meters: &Meters,
) -> Result<RawToken, Error> {
    let operation = match operation {
        AuthOperation::RenewToken => "renew_token",
    };

    let status = match &result {
        Ok(_response) => "successful",
        Err(Error::RateLimit(_duration)) => "rate_limited",
        Err(_e) => "error",
    };

    let url = url.to_string();
    let attributes = [
        KeyValue::new("url", url),
        KeyValue::new("operation", operation),
        KeyValue::new("status", status),
    ];
    meters.auth_requests.add(1, &attributes);
    result
}

#[derive(Debug)]
enum OtherOperation {
    GetProducts,
}

#[tracing::instrument]
fn increment_other_count<U: std::fmt::Debug>(
    url: &str,
    operation: OtherOperation,
    result: Result<U, Error>,
    meters: &Meters,
) -> Result<U, Error> {
    let operation = match operation {
        OtherOperation::GetProducts => "get_products",
    };

    let status = match &result {
        Ok(_response) => "successful",
        Err(Error::RateLimit(_duration)) => "rate_limited",
        Err(_e) => "error",
    };

    let url = url.to_string();
    let attributes = [
        KeyValue::new("url", url),
        KeyValue::new("operation", operation),
        KeyValue::new("status", status),
    ];
    meters.other_requests.add(1, &attributes);
    result
}

#[derive(Debug)]
enum VehicleOperation {
    WakeUp,
    SetChargeLimit,
    GetChargeState,
    ChargeStart,
    ChargeStop,
}

#[tracing::instrument]
fn increment_vehicle_count<U: std::fmt::Debug>(
    url: &str,
    operation: VehicleOperation,
    vehicle_id: VehicleId,
    result: Result<U, Error>,
    meters: &Meters,
) -> Result<U, Error> {
    let operation = match operation {
        VehicleOperation::WakeUp => "wake_up",
        VehicleOperation::SetChargeLimit => "set_charge_limit",
        VehicleOperation::ChargeStart => "charge_start",
        VehicleOperation::ChargeStop => "charge_stop",
        VehicleOperation::GetChargeState => "get_charge_state",
    };

    let status = match &result {
        Ok(_response) => "successful",
        Err(Error::RateLimit(_duration)) => "rate_limited",
        Err(_e) => "error",
    };

    let url = url.to_string();
    let attributes = [
        KeyValue::new("url", url),
        KeyValue::new("operation", operation),
        KeyValue::new("vehicle_id", vehicle_id.to_string()),
        KeyValue::new("status", status),
    ];
    meters.vehicle_requests.add(1, &attributes);
    result
}

#[tracing::instrument]
async fn post<T: Serialize + Sync + std::fmt::Debug, U: DeserializeOwned>(
    url: &str,
    body: &T,
) -> Result<U, Error> {
    debug!("post {}", url);

    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .pipe(handle_error)?;

    let text = response.json().await?;
    debug!("post done {}", url);
    Ok(text)
}

#[tracing::instrument]
#[allow(dead_code)]
async fn get<U: DeserializeOwned>(url: &str, counter: &Counter<u64>) -> Result<U, Error> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .pipe(handle_error)?;

    let text = response.json().await?;
    Ok(text)
}

#[tracing::instrument(skip(token))]
async fn get_with_token<U: DeserializeOwned>(url: &str, token: &str) -> Result<U, Error> {
    debug!("get_with_token: {}", url);

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .pipe(handle_error)?;

    // let text = response.text().await?;
    // println!("{}", text);
    // let text = serde_json::from_str(&text).unwrap();

    let text = response.json().await?;
    debug!("get_with_token done: {}", url);
    Ok(text)
}

#[tracing::instrument(skip(token))]
async fn post_with_token<T: Serialize + Sync + std::fmt::Debug, U: DeserializeOwned>(
    url: &str,
    token: &str,
    body: &T,
) -> Result<U, Error> {
    debug!("post_with_token: {}", url);

    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .json(body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .pipe(handle_error)?;

    // let text = response.text().await?;
    // println!("{}", text);
    // let text = serde_json::from_str(&text).unwrap();

    let text = response.json().await?;
    debug!("post_with_token done: {}", url);
    Ok(text)
}

#[derive(Serialize)]
struct TokenRenew {
    grant_type: String,
    client_id: String,
    refresh_token: String,
    scope: String,
}

impl std::fmt::Debug for TokenRenew {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenRenew")
            .field("grant_type", &self.grant_type)
            .field("client_id", &self.client_id)
            .field("refresh_token", &"[censored]")
            .field("scope", &self.scope)
            .finish()
    }
}

/// Raw Tesla token from API
#[derive(Deserialize)]
pub struct RawToken {
    access_token: String,
    refresh_token: String,
    id_token: String,
    token_type: String,
    expires_in: u64,
}

impl std::fmt::Debug for RawToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawToken")
            .field("access_token", &"[censored]")
            .field("refresh_token", &"[censored]")
            .field("id_token", &"[censored]")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

/// Token to access the Tesla API
#[derive(Clone, Serialize, Deserialize)]
pub struct Token {
    access_token: String,
    refresh_token: String,
    id_token: String,
    token_type: String,

    /// Time we should renew the token.
    pub renew_at: DateTime<Utc>,

    /// Time when the token expires.
    pub expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Token")
            .field("access_token", &"[censored]")
            .field("refresh_token", &"[censored]")
            .field("id_token", &"[censored]")
            .field("token_type", &self.token_type)
            .field("renew_at", &self.renew_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Serialize, Debug)]
struct SetChargeLimit {
    percent: u8,
}

/// Vehicle information
#[derive(Deserialize, Debug)]
pub struct Vehicle {
    /// Vehicle ID for owner-api endpoint.
    pub id: VehicleId,
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

    /// Is the car plugged in?
    #[must_use]
    pub const fn is_plugged_in(self) -> bool {
        match self {
            ChargingStateEnum::Starting
            | ChargingStateEnum::Charging
            | ChargingStateEnum::Complete
            | ChargingStateEnum::NoPower
            | ChargingStateEnum::Stopped => true,
            ChargingStateEnum::Disconnected => false,
        }
    }
}

/// Charging state error
#[derive(Debug, Error)]
pub enum ChargingStateError {
    /// Invalid charging state.
    #[error("Invalid charging state: {0}")]
    InvalidChargingState(String),

    /// Invalid UTF-8.
    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),
}

impl TryFrom<MqttMessage> for ChargingStateEnum {
    type Error = ChargingStateError;
    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        let payload = msg.payload_as_str();
        match payload {
            Ok("Disconnected") => Ok(Self::Disconnected),
            Ok("Charging") => Ok(Self::Charging),
            Ok("NoPower") => Ok(Self::NoPower),
            Ok("Complete") => Ok(Self::Complete),
            Ok("Stopped") => Ok(Self::Stopped),
            Ok("Starting") => Ok(Self::Starting),
            Ok(state) => Err(ChargingStateError::InvalidChargingState(state.to_string())),
            Err(err) => Err(err.into()),
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
#[derive(Debug, Deserialize)]
pub struct GenericResponse {
    /// The reason for an error or ""
    reason: String,

    /// The result of the request
    result: bool,
}

/// The response from a wake up request
#[derive(Debug, Deserialize)]
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

    /// Json error
    #[error("Json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Rate limit error
    #[error("Rate limit error, retry in: {}", duration::to_string(.0))]
    RateLimit(Duration),
}

impl From<Error> for TokenError {
    fn from(error: Error) -> Self {
        match error {
            Error::Reqwest(e) => TokenError::Reqwest(e),
            Error::Json(e) => TokenError::Json(e),
            Error::RateLimit(duration) => TokenError::RateLimit(duration),
        }
    }
}

/// A generic error return from the API
#[derive(Debug, Error)]
pub enum ApiError {
    /// Reqwest error
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Json error
    #[error("Json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Rate limit error
    #[error("Rate limit error, retry in: {}", duration::to_string(.0))]
    RateLimit(Duration),

    /// The HTTP request succeeded, but the response was not successful.
    #[error("Generic tesla error: {0}")]
    Failed(String),
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        match error {
            Error::Reqwest(e) => ApiError::Reqwest(e),
            Error::Json(e) => ApiError::Json(e),
            Error::RateLimit(duration) => ApiError::RateLimit(duration),
        }
    }
}

impl From<OuterGenericResponse> for Result<(), ApiError> {
    fn from(response: OuterGenericResponse) -> Self {
        if response.response.result {
            Ok(())
        } else if response.response.reason.is_empty() {
            Err(ApiError::Failed("no reason".into()))
        } else {
            Err(ApiError::Failed(response.response.reason))
        }
    }
}

/// An error occurred while trying to wake up the car
#[derive(Debug, Error)]
pub enum WakeupError {
    /// wait and retry error
    #[error("Wait & Retry in: {}", duration::to_string(.0))]
    WaitRetry(Duration),
}

/// An error occurred while running a sequence of commands
#[derive(Debug, Error)]
pub enum SequenceError {
    /// Reqwest error
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Json error
    #[error("Json error: {0}")]
    Json(#[from] serde_json::Error),

    /// wait and retry error
    #[error("Wait & Retry in: {}", duration::to_string(.0))]
    WaitRetry(Duration),

    /// The HTTP request succeeded, but the response was not successful.
    #[error("Generic tesla error: {0}")]
    Failed(String),
}

impl From<WakeupError> for SequenceError {
    fn from(error: WakeupError) -> Self {
        match error {
            WakeupError::WaitRetry(duration) => SequenceError::WaitRetry(duration),
        }
    }
}

impl From<ApiError> for SequenceError {
    fn from(error: ApiError) -> Self {
        match error {
            ApiError::Reqwest(e) => SequenceError::Reqwest(e),
            ApiError::Json(e) => SequenceError::Json(e),
            ApiError::RateLimit(duration) => SequenceError::WaitRetry(duration),
            ApiError::Failed(e) => SequenceError::Failed(e),
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

    const DEFAULT_EXPIRES_TIME: TimeDelta = unsafe_time_delta!(minutes: 1);
    const DEFAULT_RENEW_TIME: TimeDelta = unsafe_time_delta!(minutes: 1);

    #[tracing::instrument]
    async fn renew(&self, meters: &Meters) -> Result<Self, Error> {
        let url = "https://auth.tesla.com/oauth2/v3/token";
        let body = TokenRenew {
            grant_type: "refresh_token".into(),
            client_id: "ownerapi".into(),
            refresh_token: self.refresh_token.clone(),
            scope: "openid email offline_access".into(),
        };

        let token: RawToken = post(url, &body)
            .await
            .pipe(|result| increment_auth_count(url, AuthOperation::RenewToken, result, meters))?;

        let token = {
            let expires_in = Duration::from_secs(token.expires_in);
            let renew_in = expires_in
                .checked_sub(Duration::from_secs(60 * 60))
                .unwrap_or_default();

            let expires_in =
                chrono::Duration::from_std(expires_in).unwrap_or(Self::DEFAULT_EXPIRES_TIME);
            let renew_in = chrono::Duration::from_std(renew_in).unwrap_or(Self::DEFAULT_RENEW_TIME);

            let now = chrono::Utc::now();
            let renew_at = now + renew_in;
            let expires_at = now + expires_in;

            Token {
                access_token: token.access_token,
                refresh_token: token.refresh_token,
                id_token: token.id_token,
                token_type: token.token_type,
                renew_at,
                expires_at,
            }
        };

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
    #[tracing::instrument(skip(ps))]
    pub async fn check(
        &mut self,
        ps: &PersistentStateRow<Token>,
        meters: &Meters,
    ) -> Result<(), TokenError> {
        if self.renew_at > chrono::Utc::now() {
            return Ok(());
        }

        let token = self.renew(meters).await?;
        token.put(ps)?;
        *self = token;
        Ok(())
    }

    /// Try to wake up the car
    ///
    /// # Errors
    ///
    /// Returns `WakeupError::Reqwest` if the HTTP request failed.
    #[tracing::instrument]
    pub async fn wake_up(&self, id: VehicleId, meters: &Meters) -> Result<WakeUpResponse, Error> {
        let url = format!(
            "https://owner-api.teslamotors.com/api/1/vehicles/{id}/wake_up",
            id = id.to_string()
        );
        let response: OuterWakeUpResponse = post_with_token(&url, &self.access_token, &())
            .await
            .pipe(|result| {
                increment_vehicle_count(&url, VehicleOperation::WakeUp, id, result, meters)
            })?;
        Ok(response.response)
    }

    /// Get all the cars for the current token
    ///
    /// # Errors
    ///
    /// Returns error if the HTTP request failed.
    #[tracing::instrument]
    pub async fn get_products(&self, meters: &Meters) -> Result<Vec<Vehicle>, Error> {
        let url = "https://owner-api.teslamotors.com/api/1/products";
        let response: OuterVehiclesResponse =
            get_with_token(url, &self.access_token)
                .await
                .pipe(|result| {
                    increment_other_count(url, OtherOperation::GetProducts, result, meters)
                })?;
        Ok(response.response)
    }

    /// Wait for the car to wake up
    ///
    /// # Errors
    ///
    /// Returns `WakeupError::Reqwest` if the HTTP request failed.
    /// Returns `WakeupError::Timeout` if the car didn't wake up before the timeout elapsed.
    #[tracing::instrument]
    pub async fn wake_up_and_process_response(
        &self,
        id: VehicleId,
        meters: &Meters,
    ) -> Result<(), WakeupError> {
        info!("Trying to wake up");
        let response = self.wake_up(id, meters).await;

        match response {
            // Car is awake
            Ok(response) if response.state == "online" => {
                info!("Trying to wake up: Car is online");
                Ok(())
            }

            // Car is not awake yet
            Ok(_) => {
                info!("Trying to wake up: Car is not online yet");
                Err(WakeupError::WaitRetry(Duration::from_secs(5)))
            }

            Err(Error::Reqwest(err)) => {
                error!("Trying to wake up: Reqwest error: {}", err);
                Err(WakeupError::WaitRetry(Duration::from_secs(5)))
            }

            // Rate limited by Tesla
            Err(Error::RateLimit(duration)) => {
                info!(
                    "Trying to wake up: rate limit, retry in: {}",
                    duration::to_string(&duration)
                );
                Err(WakeupError::WaitRetry(duration))
            }

            // This should never happen
            Err(Error::Json(err)) => {
                error!("Trying to wake up: Json error (should not happen): {}", err);
                Err(WakeupError::WaitRetry(Duration::from_secs(60)))
            }
        }
    }

    /// Request the car start charging
    ///
    /// # Errors
    ///
    /// Returns `GenericError::Reqwest` if the HTTP request failed.
    /// Returns `GenericError::Failed` if the request was not successful.
    #[tracing::instrument]
    pub async fn charge_start(&self, id: VehicleId, meters: &Meters) -> Result<(), ApiError> {
        let url = format!(
            "https://owner-api.teslamotors.com/api/1/vehicles/{id}/command/charge_start",
            id = id.to_string()
        );
        let response: OuterGenericResponse = post_with_token(&url, &self.access_token, &())
            .await
            .pipe(|result| {
            increment_vehicle_count(&url, VehicleOperation::ChargeStart, id, result, meters)
        })?;
        response.into()
    }

    /// Request the car stop charging
    ///
    /// # Errors
    ///
    /// Returns `GenericError::Reqwest` if the HTTP request failed.
    /// Returns `GenericError::Failed` if the request was not successful.
    #[tracing::instrument]
    pub async fn charge_stop(&self, id: VehicleId, meters: &Meters) -> Result<(), ApiError> {
        let url = format!(
            "https://owner-api.teslamotors.com/api/1/vehicles/{id}/command/charge_stop",
            id = id.to_string()
        );
        let response: OuterGenericResponse = post_with_token(&url, &self.access_token, &())
            .await
            .pipe(|result| {
            increment_vehicle_count(&url, VehicleOperation::ChargeStop, id, result, meters)
        })?;
        response.into()
    }

    /// Set the charge limit for the car
    ///
    /// # Errors
    ///
    /// Returns `GenericError::Reqwest` if the HTTP request failed.
    /// Returns `GenericError::Failed` if the request was not successful.
    #[tracing::instrument]
    pub async fn set_charge_limit(
        &self,
        id: VehicleId,
        percent: u8,
        meters: &Meters,
    ) -> Result<(), ApiError> {
        let url = format!(
            "https://owner-api.teslamotors.com/api/1/vehicles/{id}/command/set_charge_limit",
            id = id.to_string()
        );
        let body = SetChargeLimit { percent };
        let response: OuterGenericResponse = post_with_token(&url, &self.access_token, &body)
            .await
            .pipe(|result| {
                increment_vehicle_count(&url, VehicleOperation::SetChargeLimit, id, result, meters)
            })?;

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
    #[tracing::instrument]
    pub async fn get_charge_state(
        &self,
        id: VehicleId,
        meters: &Meters,
    ) -> Result<ChargeState, Error> {
        let url = format!(
            "https://owner-api.teslamotors.com/api/1/vehicles/{id}/data_request/charge_state",
            id = id.to_string()
        );
        let response: OuterChargeState =
            get_with_token(&url, &self.access_token)
                .await
                .pipe(|result| {
                    increment_vehicle_count(
                        &url,
                        VehicleOperation::GetChargeState,
                        id,
                        result,
                        meters,
                    )
                })?;
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
    async fn execute(
        &self,
        token: &Token,
        id: VehicleId,
        meters: &Meters,
    ) -> Result<(), SequenceError> {
        match self {
            Command::WakeUp => token.wake_up_and_process_response(id, meters).await?,
            Command::SetChargeLimit(percent) => {
                token.set_charge_limit(id, *percent, meters).await?;
            }
            Command::ChargeStart => token.charge_start(id, meters).await?,
            Command::ChargeStop => token.charge_stop(id, meters).await?,
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
    #[tracing::instrument]
    pub async fn execute(
        &self,
        token: &Token,
        car_id: VehicleId,
        meters: &Meters,
    ) -> Result<(), SequenceError> {
        if self.commands.is_empty() {
            return Ok(());
        }

        if is_debug_mode() {
            debug!("Would execute commands: {:?}", self.commands);
            return Ok(());
        }

        for command in &self.prefix_commands {
            command.execute(token, car_id, meters).await?;
        }

        for command in &self.commands {
            command.execute(token, car_id, meters).await?;
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
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::similar_names)]
    use std::path::PathBuf;

    use crate::services::persistent_state::PersistentStateDatabase;

    use super::*;

    #[ignore = "requires secrets"]
    #[tokio::test]
    async fn test_get_token() {
        let meters = Meters::new();

        let state_path = PathBuf::from("state");
        let config = persistent_state::Config { state_path };
        let psd = PersistentStateDatabase::new(&config).unwrap();
        let psr = psd.for_name("tesla_token");

        let token = Token::get(&psr).unwrap();

        let token = token.renew(&meters).await.unwrap();
        // token.wait_for_wake_up(&id.to_string()).await.unwrap();
        // token.charge_start(id).await.unwrap();
        // token.charge_stop(id).await.unwrap();
        // token.set_charge_limit(id, 88).await.unwrap();
        let vehicles = token.get_products(&meters).await.unwrap();
        println!("{vehicles:#?}");

        // let charge_state = token.get_charge_state(id).await.unwrap();
        // println!("{charge_state:#?}");

        token.put(&psr).unwrap();
    }
}
