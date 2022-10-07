//! Source for life360 based data
use anyhow::anyhow;
use anyhow::Result;
use log::{error, log, Level};
use serde::Deserialize;
use serde::Serialize;
use std::cmp::min;
use std::{env, time::Duration};
use tokio::time::MissedTickBehavior;

use tokio::time;

use crate::entities;
use crate::spawn;

#[derive(Deserialize)]
struct Login {
    access_token: String,
    // token_type: String,
    // onboarding: usize,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListItem {
    id: String,
    // name: String,
    // color: String,
    // #[serde(rename = "type")]
    // circle_type: String,
    // created_at: String,
    // member_count: String,
    // unread_messages: String,
    // unread_notifications: String,
    // features
}

#[derive(Deserialize, Debug)]
struct List {
    circles: Vec<ListItem>,
}

/// Life360 location struct
#[allow(missing_docs)]
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub latitude: String,
    pub longitude: String,
    pub accuracy: String,
    // start_timestamp: u64,
    pub end_timestamp: String,
    // since: u64,
    pub timestamp: String,
    pub name: Option<String>,
    pub place_type: Option<String>,
    pub source: Option<String>,
    pub source_id: Option<String>,
    pub address1: Option<String>,
    pub address2: Option<String>,
    pub short_address: String,
    pub in_transit: String,
    pub trip_id: Option<String>,
    #[serde(rename = "DriveSDKStatus")]
    pub drive_sdk_status: Option<String>,
    pub battery: String,
    pub charge: String,
    pub wifi_state: String,
    pub speed: f32,
    pub is_driving: String,
    pub user_activity: Option<String>,
}

/// Life360 communication struct
#[allow(missing_docs)]
#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Communication {
    pub channel: String,
    pub value: String,
    #[serde(rename = "type")]
    pub circle_type: Option<String>,
}

/// Life360 member struct
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct Member {
    // features
    // issues
    pub location: Location,
    pub communications: Vec<Communication>,
    pub medical: Option<String>,
    pub relation: Option<String>,
    pub created_at: String,
    pub activity: Option<String>,
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub is_admin: String,
    pub avatar: String,
    pub pin_number: Option<String>,
    pub login_email: String,
    pub login_phone: String,
}

impl PartialEq for Member {
    fn eq(&self, other: &Self) -> bool {
        self.location.timestamp == other.location.timestamp
            && self.communications == other.communications
            && self.medical == other.medical
            && self.relation == other.relation
            && self.created_at == other.created_at
            && self.activity == other.activity
            && self.id == other.id
            && self.first_name == other.first_name
            && self.last_name == other.last_name
            && self.is_admin == other.is_admin
            && self.avatar == other.avatar
            && self.pin_number == other.pin_number
            && self.login_email == other.login_email
            && self.login_phone == other.login_phone
    }
}

impl Eq for Member {}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct Circle {
    // id: String,
    // name: String,
    // color: String,
    // #[serde(rename = "type")]
    // circle_type: String,
    // created_at: String,
    // member_count: String,
    // unread_messages: String,
    // unread_notifications: String,
    // features
    members: Vec<Member>,
}

/// Source of life360 member information.
#[must_use]
pub fn circles(name: &str) -> entities::Receiver<Vec<Member>> {
    let (tx, rx) = entities::create_entity(name);
    let username = env::var("LIFE360_USERNAME").expect("LIFE360_USERNAME should be set");
    let password = env::var("LIFE360_PASSWORD").expect("LIFE360_PASSWORD should be set");

    spawn(async move {
        let login = retry_login(&username, &password).await;
        let mut interval = time::interval(Duration::from_secs(15));
        let mut refresh_interval = time::interval(Duration::from_secs(60 * 5));
        let mut circles: Option<List> = None;
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        refresh_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = refresh_interval.tick() => {
                    circles = get_circles_or_none(&login).await;
                }

                _ = interval.tick() => {
                    if circles.is_none() {
                        circles = get_circles_or_none(&login).await;
                    }
                    if let Some(circles) = &circles {
                        dispatch_circle_details(&login, circles, &tx).await;
                    }
                }
            }
        }
    });

    rx
}

async fn retry_login(username: &str, password: &str) -> Login {
    let mut attempt: u32 = 0;

    loop {
        let sleep_time = 1000 * 2u64.checked_pow(attempt).unwrap();
        let sleep_time = min(60_000, sleep_time);

        let log_level = if attempt == 0 {
            Level::Debug
        } else {
            Level::Warn
        };

        log!(
            log_level,
            "Waiting {sleep_time} ms to retry connection attempt {attempt}."
        );
        tokio::time::sleep(Duration::from_millis(sleep_time)).await;

        log!(log_level, "Trying to login");
        let login_or_none = match login(username, password).await {
            Err(err) => {
                error!("login: {err}");
                None
            }
            Ok(login) => Some(login),
        };

        if let Some(login) = login_or_none {
            log!(log_level, "Successfully logged in");
            break login;
        }

        attempt = attempt.saturating_add(1);
    }
}

async fn get_circles_or_none(login: &Login) -> Option<List> {
    match get_circles(login).await {
        Err(err) => {
            error!("get_circles: {err}");
            None
        }
        Ok(c) => Some(c),
    }
}

async fn dispatch_circle_details(
    login: &Login,
    circles: &List,
    tx: &entities::Sender<Vec<Member>>,
) {
    for circle in &circles.circles {
        match get_circle_details(login, circle).await {
            Err(err) => error!("get_circle_details: {err}"),
            Ok(details) => {
                tx.send(details.members).await;
            }
        }
    }
}

async fn login(username: &str, password: &str) -> Result<Login> {
    let url = "https://www.life360.com/v3/oauth2/token";
    let params = [
        ("username", username),
        ("password", password),
        ("grant_type", "password"),
    ];

    let client = reqwest::Client::new();
    let response = client.post(url)
        .header("accept", "application/json")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("authorization", "Basic U3dlcUFOQWdFVkVoVWt1cGVjcmVrYXN0ZXFhVGVXckFTV2E1dXN3MzpXMnZBV3JlY2hhUHJlZGFoVVJhZ1VYYWZyQW5hbWVqdQ==")
        .form(&params)
        .timeout(Duration::from_secs(30))
        .send().await?;

    let response = response.error_for_status()?;
    let payload = response.text().await?;

    let d = &mut serde_json::Deserializer::from_str(&payload);
    let login: Login = serde_path_to_error::deserialize(d).map_err(|e| anyhow!("login: {e}"))?;

    Ok(login)
}

async fn get_circles(login: &Login) -> Result<List> {
    let url = "https://www.life360.com/v3/circles";
    let token = &login.access_token;

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("accept", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .timeout(Duration::from_secs(30))
        .send()
        .await?;

    let response = response.error_for_status()?;
    let payload = response.text().await?;

    let d = &mut serde_json::Deserializer::from_str(&payload);
    let list: List =
        serde_path_to_error::deserialize(d).map_err(|e| anyhow!("get_circles: {e}"))?;

    Ok(list)
}

async fn get_circle_details(login: &Login, circle: &ListItem) -> Result<Circle> {
    let url = format!("https://www.life360.com/v3/circles/{}", circle.id);
    let token = &login.access_token;

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("accept", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .timeout(Duration::from_secs(30))
        .send()
        .await?;

    let response = response.error_for_status()?;
    let payload = response.text().await?;

    let d = &mut serde_json::Deserializer::from_str(&payload);
    let circle: Circle =
        serde_path_to_error::deserialize(d).map_err(|e| anyhow!("get_circle_details: {e}"))?;

    Ok(circle)
}
