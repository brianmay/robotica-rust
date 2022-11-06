//! Get information from Amber electricity supplier

use chrono::{TimeZone, Utc};
use influxdb::InfluxDbWriteable;
use log::debug;
use serde::Deserialize;
use thiserror::Error;
use tokio::time::{interval, MissedTickBehavior};

use robotica_backend::{get_env, is_debug_mode, spawn, EnvironmentError};
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

/// Get the current electricity price from Amber
///
/// # Errors
///
/// Returns an `AmberError` if the required environment variables are not set.
///
pub fn run<T>(timezone: T) -> Result<(), AmberError>
where
    T: TimeZone + Send + 'static,
{
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

    spawn(async move {
        // Update prices every 5 minutes
        let mut price_interval = interval(tokio::time::Duration::from_secs(300));
        price_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // Update usage once an hour
        let mut usage_interval = interval(tokio::time::Duration::from_secs(3600));
        usage_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = price_interval.tick() => {
                    let now = utc_now();
                    let today = now.with_timezone::<T>(&timezone).date();
                    let yesterday = today - Duration::days(1);
                    let tomorrow = today + Duration::days(1);
                        process_prices(&config, yesterday, tomorrow).await;
                }
                _ = usage_interval.tick() => {
                    let now = utc_now();
                    let today = now.with_timezone::<T>(&timezone).date();
                    let yesterday = today - Duration::days(1);
                    let tomorrow = today + Duration::days(1);
                    process_usage(&config, yesterday, tomorrow).await;
                }
            }
        }
    });

    Ok(())
}

#[allow(clippy::enum_variant_names)]
#[derive(Deserialize, Debug)]
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

async fn process_prices(config: &Config, start_date: Date, end_date: Date) {
    let prices = get_prices(config, start_date, end_date).await;
    match prices {
        Ok(prices) => {
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
                    time: data.start_time.into(),
                }
                .into_query("amber/price");

                if let Err(e) = client.query(&reading).await {
                    log::error!("Failed to write to influxdb: {}", e);
                }
            }
        }
        Err(e) => {
            log::error!("Failed to get prices: {}", e);
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
