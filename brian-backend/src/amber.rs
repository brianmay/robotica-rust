//! Get information from Amber electricity supplier

use chrono::{FixedOffset, Utc};
use influxdb::InfluxDbWriteable;
use log::debug;
use serde::Deserialize;
use thiserror::Error;
use tokio::time::{interval, MissedTickBehavior};

use robotica_backend::{
    entities::{self, Receiver, StatefulData},
    get_env, is_debug_mode, spawn, EnvironmentError,
};
use robotica_common::datetime::{utc_now, Date, DateTime, Duration};

/// Error when starting the Amber service
#[derive(Error, Debug)]
pub enum AmberError {
    /// Environment variable not found
    #[error("Environment variable error: {0}")]
    EnvironmentError(#[from] EnvironmentError),
}

struct Config {
    token: String,
    site_id: String,
    influx_url: String,
    influx_database: String,
}

#[derive(InfluxDbWriteable)]
struct PriceReading {
    duration: u16,
    per_kwh: f32,
    renewables: f32,
    time: chrono::DateTime<Utc>,
}

#[derive(InfluxDbWriteable)]
struct UsageReading {
    duration: u16,
    per_kwh: f32,
    renewables: f32,
    kwh: f32,
    cost: f32,
    time: chrono::DateTime<Utc>,
}

const HOURS_TO_SECONDS: u16 = 3600;
fn hours(num: u16) -> u16 {
    num * HOURS_TO_SECONDS
}

/// Get the current electricity price from Amber
///
/// # Errors
///
/// Returns an `AmberError` if the required environment variables are not set.
///
pub fn run() -> Result<Receiver<StatefulData<PriceSummary>>, AmberError> {
    let token = get_env("AMBER_TOKEN")?;
    let site_id = get_env("AMBER_SITE_ID")?;
    let influx_url = get_env("INFLUXDB_URL")?;
    let influx_database = get_env("INFLUXDB_DATABASE")?;
    let config = Config {
        token,
        site_id,
        influx_url,
        influx_database,
    };

    let (tx, rx) = entities::create_stateful_entity("amber_summary");

    spawn(async move {
        // if is_debug_mode() {
        //     let start_date = Date::from_ymd(2022, 1, 1);
        //     let stop_date = Date::from_ymd(2022, 3, 1);
        //     // process_prices(&config, start_date, stop_date).await;
        //     process_usage(&config, start_date, stop_date).await;
        //     println!("------------------- done -------------------");
        // }

        let nem_timezone = FixedOffset::east(hours(10).into());

        // Update prices every 5 minutes
        let mut price_interval = interval(tokio::time::Duration::from_secs(300));
        price_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // Update usage once an hour
        let mut usage_interval = interval(tokio::time::Duration::from_secs(hours(1).into()));
        usage_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = price_interval.tick() => {
                    let now = utc_now();
                    let today = now.with_timezone(&nem_timezone).date();
                    let yesterday = today - Duration::days(1);
                    let tomorrow = today + Duration::days(1);
                        let summary = process_prices(&config, yesterday, tomorrow).await;
                        if let Some(summary) = summary {
                            tx.try_send(summary);
                        }
                }
                _ = usage_interval.tick() => {
                    let now = utc_now();
                    let today = now.with_timezone(&nem_timezone).date();
                    let yesterday = today - Duration::days(1);
                    let tomorrow = today + Duration::days(1);
                    process_usage(&config, yesterday, tomorrow).await;
                }
            }
        }
    });

    Ok(rx)
}

#[allow(clippy::enum_variant_names)]
#[derive(Deserialize, Debug, Copy, Clone, Eq, PartialEq)]
enum IntervalType {
    ActualInterval,
    ForecastInterval,
    CurrentInterval,
}

#[allow(clippy::enum_variant_names)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
enum Quality {
    Estimated,
    Billable,
}

#[allow(clippy::enum_variant_names)]
#[derive(Deserialize, Debug)]
enum UsageType {
    Usage,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
enum ChannelType {
    General,
    ControlledLoad,
    FeedIn,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
enum PeriodType {
    OffPeak,
    Shoulder,
    SolarSponge,
    Peak,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
enum SeasonType {
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
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TariffInformation {
    period: PeriodType,
    season: Option<SeasonType>,
    block: Option<u32>,
    demand_window: Option<bool>,
}

/// Amber price response
#[allow(dead_code)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PriceResponse {
    #[serde(rename = "type")]
    interval_type: IntervalType,
    duration: u16,
    spot_per_kwh: f32,
    per_kwh: f32,
    date: Date,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    renewables: f32,
    channel_type: ChannelType,
    tariff_information: TariffInformation,
    spike_status: String,
    estimate: Option<bool>,
}

/// Amber usage response
#[allow(dead_code)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsageResponse {
    #[serde(rename = "type")]
    usage_type: UsageType,
    duration: u16,
    spot_per_kwh: f32,
    per_kwh: f32,
    date: Date,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    renewables: f32,
    channel_type: ChannelType,
    tariff_information: TariffInformation,
    spike_status: String,
    channel_identifier: String,
    kwh: f32,
    quality: Quality,
    cost: f32,
}

async fn get_prices(
    config: &Config,
    start_date: Date,
    end_date: Date,
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

async fn get_usage(
    config: &Config,
    start_date: Date,
    end_date: Date,
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PriceCategory {
    SuperCheap,
    Cheap,
    Normal,
    Expensive,
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PriceSummary {
    pub category: PriceCategory,
}

async fn process_prices(config: &Config, start_date: Date, end_date: Date) -> Option<PriceSummary> {
    let prices = get_prices(config, start_date, end_date).await;

    match prices {
        Ok(prices) => {
            prices_to_influxdb(config, &prices).await;
            let summary = prices_to_summary(&prices);
            Some(summary)
        }
        Err(e) => {
            log::error!("Error getting prices: {}", e);
            None
        }
    }
}

fn prices_to_summary(prices: &[PriceResponse]) -> PriceSummary {
    let mut quality: PriceCategory = PriceCategory::Normal;

    prices
        .iter()
        .filter(|p| p.interval_type == IntervalType::CurrentInterval)
        .map(|price| {
            if price.per_kwh < 10.0 {
                PriceCategory::SuperCheap
            } else if price.per_kwh < 20.0 {
                PriceCategory::Cheap
            } else if price.per_kwh < 30.0 {
                PriceCategory::Normal
            } else {
                PriceCategory::Expensive
            }
        })
        .for_each(|new_pq| quality = new_pq);

    PriceSummary { category: quality }
}

async fn prices_to_influxdb(config: &Config, prices: &[PriceResponse]) {
    let client = influxdb::Client::new(&config.influx_url, &config.influx_database);

    if is_debug_mode() {
        debug!("Skipping writing prices to influxdb in debug mode");
        return;
    }

    for data in prices {
        let reading = PriceReading {
            duration: data.duration,
            per_kwh: data.per_kwh,
            renewables: data.renewables,
            time: data.start_time.clone().into(),
        }
        .into_query("amber/price");

        if let Err(e) = client.query(&reading).await {
            log::error!("Failed to write to influxdb: {}", e);
        }
    }
}

async fn process_usage(config: &Config, start_date: Date, end_date: Date) {
    let usage = get_usage(config, start_date, end_date).await;
    match usage {
        Ok(usage) => {
            let client = influxdb::Client::new(&config.influx_url, &config.influx_database);

            if is_debug_mode() {
                debug!("Skipping writing usage to influxdb in debug mode");
                return;
            }

            for data in usage {
                let name = format!("amber/usage/{}", data.channel_identifier);
                let reading = UsageReading {
                    duration: data.duration,
                    per_kwh: data.per_kwh,
                    renewables: data.renewables,
                    kwh: data.kwh,
                    cost: data.cost,
                    time: data.start_time.into(),
                }
                .into_query(name);

                if let Err(e) = client.query(&reading).await {
                    log::error!("Failed to write to influxdb: {}", e);
                }
            }
        }
        Err(e) => {
            log::error!("Failed to get usage: {}", e);
        }
    }
}
