use super::{api, Prices, Usage};
use crate::influxdb as influx;
use chrono::{DateTime, Utc};
use influxdb::InfluxDbWriteable;
use robotica_common::datetime::utc_now;
use robotica_tokio::{
    pipes::{stateful::Receiver, stateless, Subscriber, Subscription},
    spawn,
};
use std::sync::Arc;
use tokio::select;
use tracing::error;

pub fn log_prices(
    rx_prices: Receiver<Arc<Prices>>,
    rx_tick: stateless::Receiver<DateTime<Utc>>,
    influxdb_config: &influx::Config,
) {
    let influxdb_config = influxdb_config.clone();

    spawn(async move {
        let mut s_prices = rx_prices.subscribe().await;
        let mut s_tick = rx_tick.subscribe().await;
        let mut last_prices: Option<Arc<Prices>> = None;

        loop {
            select! {
                Ok(prices) = s_prices.recv() => {
                    price_readings_to_influxdb(&influxdb_config, &prices).await;
                    price_summaries_to_influxdb(&influxdb_config, &prices).await;
                    last_prices = Some(prices);
                }
                // Record the current price at each interval boundary,
                // even if the price data has not changed.
                Ok(_boundary) = s_tick.recv() => {
                    if let Some(prices) = &last_prices {
                        price_summaries_to_influxdb(&influxdb_config, prices).await;
                    }
                }
                else => break,
            }
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
    interval_type: String,
}

const fn interval_type_name(interval_type: api::IntervalType) -> &'static str {
    match interval_type {
        api::IntervalType::ActualInterval => "actual",
        api::IntervalType::ForecastInterval => "forecast",
        api::IntervalType::CurrentInterval => "current",
    }
}

#[derive(InfluxDbWriteable)]
struct PriceSummarySummary {
    per_kwh: f32,
    channel_type: String,
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

async fn price_readings_to_influxdb(influxdb_config: &influx::Config, prices: &Prices) {
    let client = influxdb_config.get_client();

    for data in prices.combined_iter() {
        let reading = PriceReading {
            duration: data.duration,
            per_kwh: data.effective_per_kwh(),
            renewables: data.renewables,
            time: data.start_time,
            interval_type: interval_type_name(data.interval_type).to_string(),
        };

        let measurement = match data.channel_type {
            api::ChannelType::FeedIn => "amber/price_feed_in",
            api::ChannelType::ControlledLoad => "amber/price_controlled_load",
            api::ChannelType::General => "amber/price",
        };

        match reading.try_into_query(measurement) {
            Ok(query) => {
                if let Err(e) = client.query(&query).await {
                    error!("Failed to write to influxdb: {}", e);
                }
            }
            Err(_) => {
                error!("Failed to create influxdb query");
            }
        }
    }
}

async fn price_summaries_to_influxdb(influxdb_config: &influx::Config, prices: &Prices) {
    let client = influxdb_config.get_client();
    let now = utc_now();

    let current_prices = prices.current(&now);
    for current in &current_prices {
        let reading = PriceSummarySummary {
            per_kwh: current.effective_per_kwh(),
            channel_type: match current.channel_type {
                api::ChannelType::General => "general".to_string(),
                api::ChannelType::ControlledLoad => "controlled_load".to_string(),
                api::ChannelType::FeedIn => "feed_in".to_string(),
            },
            time: now,
        };

        let measurement = match current.channel_type {
            api::ChannelType::FeedIn => "amber/price_feed_in_summary",
            api::ChannelType::ControlledLoad => "amber/price_controlled_load_summary",
            api::ChannelType::General => "amber/price_summary",
        };

        match reading.try_into_query(measurement) {
            Ok(query) => {
                if let Err(e) = client.query(&query).await {
                    error!("Failed to write to influxdb: {}", e);
                }
            }
            Err(_) => {
                error!("Failed to create influxdb query");
            }
        }
    }
}

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
        };

        match reading.try_into_query(format!("amber/usage/{channel}")) {
            Ok(query) => {
                if let Err(e) = client.query(&query).await {
                    error!("Failed to write to influxdb: {}", e);
                }
            }
            Err(_) => {
                error!("Failed to create influxdb query");
            }
        }
    }
}
