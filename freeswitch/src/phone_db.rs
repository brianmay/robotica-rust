use serde::{Deserialize, Serialize};
use tracing::error;

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

#[derive(Deserialize, Debug)]
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

pub async fn check_number(
    caller_number: &str,
    destination_number: &str,
    config: &Config,
) -> Response {
    let client = reqwest::Client::new();
    let request = Request {
        phone_number: caller_number.to_string(),
        destination_number: destination_number.to_string(),
    };

    let res = client
        .post(&config.url)
        .json(&request)
        .basic_auth(&config.username, Some(&config.password))
        .send()
        .await;

    match res {
        Ok(res) => {
            if res.status().is_success() {
                match res.json().await {
                    Ok(response) => response,
                    Err(err) => {
                        error!("JSON Error: {}", err);
                        Response {
                            name: None,
                            action: Action::Allow,
                        }
                    }
                }
            } else {
                error!("Server Error: {}", res.status());
                Response {
                    name: None,
                    action: Action::Allow,
                }
            }
        }
        Err(err) => {
            error!("HTTP Error: {}", err);
            Response {
                name: None,
                action: Action::Allow,
            }
        }
    }
}
