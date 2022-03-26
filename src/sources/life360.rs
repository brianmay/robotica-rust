use anyhow::anyhow;
use anyhow::Result;
use log::*;
use serde::Deserialize;
use serde::Serialize;
use std::{env, time::Duration};
use tokio::sync::mpsc::Sender;

use tokio::{
    sync::mpsc,
    time::{self},
};

use crate::send;

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

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Communication {
    pub channel: String,
    pub value: String,
    #[serde(rename = "type")]
    pub circle_type: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
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

pub fn circles() -> mpsc::Receiver<Member> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        let username = env::var("LIFE360_USERNAME").unwrap();
        let password = env::var("LIFE360_PASSWORD").unwrap();
        let login = login(&username, &password)
            .await
            .expect("life360 login failed");
        let mut interval = time::interval(Duration::from_secs(15));

        loop {
            if let Err(err) = do_tick(&login, &tx).await {
                error!("life360: {err}");
            }

            interval.tick().await;
        }
    });
    rx
}

async fn do_tick(login: &Login, tx: &Sender<Member>) -> Result<()> {
    let circles = get_circles(login).await?;
    for circle in circles.circles {
        let details = get_circle_details(login, &circle).await?;
        for member in details.members {
            send(tx, member).await;
        }
    }
    Ok(())
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
        .send()
        .await?;

    let response = response.error_for_status()?;
    let payload = response.text().await?;

    let d = &mut serde_json::Deserializer::from_str(&payload);
    let circle: Circle =
        serde_path_to_error::deserialize(d).map_err(|e| anyhow!("get_circle_details: {e}"))?;

    Ok(circle)
}
