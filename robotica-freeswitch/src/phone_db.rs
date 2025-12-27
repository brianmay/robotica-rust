use std::time::Duration;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Deserialize)]
pub struct Config {
    url: String,
    username: String,
    password: String,
}

#[derive(Serialize, Debug)]
struct Request {
    phone_number: String,
    destination_number: String,
}

#[derive(Deserialize, Debug, Copy, Clone)]
pub enum Action {
    #[serde(rename = "allow")]
    Allow,
    #[serde(rename = "voicemail")]
    VoiceMail,
}

#[derive(Deserialize, Debug)]
pub struct Response {
    pub name: Option<String>,
    pub action: Action,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("HTTP Error: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("Server Error: {0}")]
    ServerError(StatusCode),
}

pub async fn check_number(
    caller_number: &str,
    destination_number: &str,
    config: &Config,
) -> Result<Response, Error> {
    let client = reqwest::Client::new();
    let request = Request {
        phone_number: caller_number.to_string(),
        destination_number: destination_number.to_string(),
    };

    let res = client
        .post(&config.url)
        .json(&request)
        .basic_auth(&config.username, Some(&config.password))
        .timeout(Duration::from_secs(5))
        .send()
        .await?;

    if res.status().is_success() {
        Ok(res.json().await?)
    } else {
        Err(Error::ServerError(res.status()))
    }
}
