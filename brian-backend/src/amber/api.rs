use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub token: String,
    pub site_id: String,
}

#[allow(clippy::enum_variant_names)]
#[derive(Deserialize, Debug, Copy, Clone, Eq, PartialEq)]
pub enum IntervalType {
    ActualInterval,
    ForecastInterval,
    CurrentInterval,
}

impl From<IntervalType> for influxdb::Type {
    fn from(interval_type: IntervalType) -> Self {
        let v = match interval_type {
            IntervalType::ActualInterval => "actual",
            IntervalType::ForecastInterval => "forecast",
            IntervalType::CurrentInterval => "current",
        };
        Self::Text(v.to_string())
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    Estimated,
    Billable,
}

#[allow(clippy::enum_variant_names)]
#[derive(Deserialize, Debug)]
pub enum UsageType {
    Usage,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum ChannelType {
    General,
    ControlledLoad,
    FeedIn,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum PeriodType {
    OffPeak,
    Shoulder,
    SolarSponge,
    Peak,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum SeasonType {
    Default,
    Summer,
    Autumn,
    Winter,
    Spring,
    NonSummer,
    Holiday,
    Weekend,
    WeekendHoliday,
    Weekday,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TariffInformation {
    pub period: PeriodType,
    pub season: Option<SeasonType>,
    pub block: Option<u32>,
    pub demand_window: Option<bool>,
}

/// Amber price response
#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PriceResponse {
    #[serde(rename = "type")]
    pub interval_type: IntervalType,
    pub duration: u16,
    pub spot_per_kwh: f32,
    pub per_kwh: f32,
    pub date: NaiveDate,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub renewables: f32,
    pub channel_type: ChannelType,
    pub tariff_information: TariffInformation,
    pub spike_status: String,
    pub estimate: Option<bool>,
}

/// Amber usage response
#[allow(dead_code)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsageResponse {
    #[serde(rename = "type")]
    pub usage_type: UsageType,
    pub duration: u16,
    pub spot_per_kwh: f32,
    pub per_kwh: f32,
    pub date: NaiveDate,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub renewables: f32,
    pub channel_type: ChannelType,
    pub tariff_information: TariffInformation,
    pub spike_status: String,
    pub channel_identifier: String,
    pub kwh: f32,
    pub quality: Quality,
    pub cost: f32,
}

pub async fn get_prices(
    config: &Config,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<PriceResponse>, reqwest::Error> {
    let url = format!(
        "https://api.amber.com.au/v1/sites/{}/prices",
        config.site_id
    );

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("accept", "application/json")
        .header("authorization", format!("Bearer {}", config.token))
        .query(&[
            ("startDate", start_date.to_string()),
            ("endDate", end_date.to_string()),
        ])
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;

    response.json().await
}

pub async fn get_usage(
    config: &Config,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<UsageResponse>, reqwest::Error> {
    let url = format!("https://api.amber.com.au/v1/sites/{}/usage", config.site_id);

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("accept", "application/json")
        .header("authorization", format!("Bearer {}", config.token))
        .query(&[
            ("startDate", start_date.to_string()),
            ("endDate", end_date.to_string()),
        ])
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;

    response.json().await
}
