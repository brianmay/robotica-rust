use super::{api, Prices, Usage};
use crate::influxdb as influx;
use chrono::Utc;
use influxdb::InfluxDbWriteable;
use robotica_common::datetime::utc_now;
use robotica_tokio::{
    pipes::{stateful::Receiver, Subscriber, Subscription},
    spawn,
};
use std::sync::Arc;
use tracing::error;

pub fn log_prices(rx: Receiver<Arc<Prices>>, influxdb_config: &influx::Config) {
    let influxdb_config = influxdb_config.clone();

    spawn(async move {
        let mut s = rx.subscribe().await;

        while let Ok(prices) = s.recv().await {
            prices_to_influxdb(&influxdb_config, &prices).await;
        }
    });
}

pub fn log_usage(rx: Receiver<Arc<Usage>>, influxdb_config: &influx::Config) {
    let influxdb_config = influxdb_config.clone();

    spawn(async move {
        let mut s = rx.subscribe().await;

        while let Ok(usage) = s.recv().await {
            usage_to_influxdb(&influxdb_config, &usage).await;
        }
    });
}

#[derive(InfluxDbWriteable)]
struct PriceReading {
    duration: u16,
    per_kwh: f32,
    renewables: f32,
    time: chrono::DateTime<Utc>,
    interval_type: api::IntervalType,
}

#[derive(InfluxDbWriteable)]
struct PriceSummaryReading {
    per_kwh: f32,
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

#[allow(clippy::unwrap_used)]
/// Note: unwrap is used because the influxdb-rs crate's derive macro generates a private
/// error type. See <https://github.com/influxdb-rs/influxdb-rust/issues/188>
async fn prices_to_influxdb(influxdb_config: &influx::Config, prices: &Prices) {
    let client = influxdb_config.get_client();
    let now = utc_now();

    for data in &prices.list {
        let reading = PriceReading {
            duration: data.duration,
            per_kwh: data.per_kwh,
            renewables: data.renewables,
            time: data.start_time,
            interval_type: data.interval_type,
        }
        .try_into_query("amber/price")
        .unwrap();

        if let Err(e) = client.query(&reading).await {
            error!("Failed to write to influxdb: {}", e);
        }
    }

    if let Some(current) = prices.current(&now) {
        let reading = PriceSummaryReading {
            per_kwh: current.per_kwh,
            time: Utc::now(),
        }
        .try_into_query("amber/price_summary")
        .unwrap();

        if let Err(e) = client.query(&reading).await {
            error!("Failed to write to influxdb: {}", e);
        }
    }
}

#[allow(clippy::unwrap_used)]
/// Note: unwrap is used because the influxdb-rs crate's derive macro generates a private
/// error type. See <https://github.com/influxdb-rs/influxdb-rust/issues/188>
async fn usage_to_influxdb(influxdb_config: &influx::Config, usage: &Usage) {
    let client = influxdb_config.get_client();

    for data in &usage.list {
        let channel = &data.channel_identifier;

        let reading = UsageReading {
            duration: data.duration,
            per_kwh: data.per_kwh,
            renewables: data.renewables,
            kwh: data.kwh,
            cost: data.cost,
            time: data.start_time,
        }
        .try_into_query(format!("amber/usage/{channel}"))
        .unwrap();

        if let Err(e) = client.query(&reading).await {
            error!("Failed to write to influxdb: {}", e);
        }
    }
}