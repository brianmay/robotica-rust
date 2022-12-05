//! Wrapper around Tesla's API

use std::time::Duration;

use reqwest::Error;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use tokio::time::Instant;

use crate::{get_env, EnvironmentError};

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
        .header("Authorization", format!("Bearer {}", token))
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
        .header("Authorization", format!("Bearer {}", token))
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
    /// Charging is complete
    Complete,

    /// Charging is in progress
    Charging,

    /// Charging is not in progress and we are disconnected
    Disconnected,

    /// Charging is not in progress
    Stopped,
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
    /// The environment variable is not set
    #[error("Token env error: {0}")]
    Environment(#[from] EnvironmentError),

    /// Reqwest error
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// IO error loading the token
    #[error("Token IO error: {0}")]
    Io(String, std::io::Error),

    /// Error deserializing the token
    #[error("Token JSON error: {0}")]
    Json(#[from] serde_json::Error),
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

impl Token {
    /// Load token from file
    ///
    /// # Errors
    ///
    /// Returns `TokenError::Environment` if the environment variable `TESLA_SECRET_FILE` is not set.
    /// Returns `TokenError::Io` if the file could not be read.
    /// Returns `TokenError::Json` if the file could not be deserialized.
    /// Returns `TokenError::Reqwest` if the token could not be refreshed.
    pub fn get() -> Result<Self, TokenError> {
        let filename = get_env("TESLA_SECRET_FILE")?;
        let token = std::fs::read_to_string(&filename).map_err(|e| TokenError::Io(filename, e))?;
        let token: Token = serde_json::from_str(&token)?;
        Ok(token)
    }

    fn put(&self) -> Result<(), TokenError> {
        let filename = get_env("TESLA_SECRET_FILE")?;
        let token = serde_json::to_string(&self)?;
        std::fs::write(&filename, token).map_err(|e| TokenError::Io(filename, e))?;
        Ok(())
    }

    async fn renew(&self) -> Result<Self, Error> {
        // let oauth = Oauth::get().await?;
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
    pub async fn check(&mut self) -> Result<(), TokenError> {
        if let Some(renew_at) = self.expires_at {
            if renew_at > Instant::now() {
                return Ok(());
            }
        }

        let token = self.renew().await?;
        token.put()?;
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

        while Instant::now() < timeout {
            let response = self.wake_up(id).await?;
            if response.state == "online" {
                return Ok(());
            }
            std::thread::sleep(Duration::from_secs(5));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "requires secrets"]
    #[tokio::test]
    async fn test_get_token() {
        let id = 1_492_931_337_154_343u64;

        let token = Token::get().unwrap();
        let token = token.renew().await.unwrap();
        // token.wait_for_wake_up(&id.to_string()).await.unwrap();
        // token.charge_start(id).await.unwrap();
        // token.charge_stop(id).await.unwrap();
        // token.set_charge_limit(id, 88).await.unwrap();
        let vehicles = token.get_vehicles().await.unwrap();
        println!("{:#?}", vehicles);

        let charge_state = token.get_charge_state(id).await.unwrap();
        println!("{:#?}", charge_state);

        token.put().unwrap();
    }
}
