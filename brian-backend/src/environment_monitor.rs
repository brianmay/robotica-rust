use chrono::{DateTime, Utc};
use influxdb::{Client, InfluxDbWriteable};

use robotica_backend::{get_env, is_debug_mode, spawn, EnvironmentError};
use robotica_common::anavi_thermometer::{self as anavi, GetReading};
use robotica_common::mqtt::MqttMessage;
use serde::Deserialize;
use tracing::{debug, error};

use crate::State;

#[derive(InfluxDbWriteable)]
struct Reading {
    value: f64,
    time: DateTime<Utc>,
}

#[derive(Deserialize, Clone, Debug)]
struct FishTankData {
    distance: u16,
    temperature: f32,
    tds: f32,
}

impl TryFrom<MqttMessage> for FishTankData {
    type Error = serde_json::Error;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        serde_json::from_str(&msg.payload)
    }
}

#[derive(InfluxDbWriteable)]
struct FishTankReading {
    distance: u16,
    temperature: f32,
    tds: f32,
    time: DateTime<Utc>,
}

pub fn monitor_float_value<T>(state: &mut State, topic: &str) -> Result<(), EnvironmentError>
where
    T: TryFrom<MqttMessage> + Clone + Send + 'static + GetReading,
    <T as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
{
    let rx = state.subscriptions.subscribe_into_stateless::<T>(topic);
    let topic = topic.to_string();
    let influx_url = get_env("INFLUXDB_URL")?;
    let influx_database = get_env("INFLUXDB_DATABASE")?;

    spawn(async move {
        let client = Client::new(&influx_url, &influx_database);
        let mut s = rx.subscribe().await;

        while let Ok(data) = s.recv().await {
            let value = data.get_reading();
            let reading = Reading {
                value,
                time: Utc::now(),
            }
            .into_query(&topic);

            if is_debug_mode() {
                debug!("would send {:?}", reading);
            } else if let Err(e) = client.query(&reading).await {
                error!("Failed to write to influxdb: {}", e);
            }
        }
    });

    Ok(())
}

pub fn monitor_fishtank(state: &mut State, topic: &str) -> Result<(), EnvironmentError> {
    let rx = state
        .subscriptions
        .subscribe_into_stateless::<FishTankData>(topic);
    let topic = topic.to_string();
    let influx_url = get_env("INFLUXDB_URL")?;
    let influx_database = get_env("INFLUXDB_DATABASE")?;

    spawn(async move {
        let client = Client::new(&influx_url, &influx_database);
        let mut s = rx.subscribe().await;

        while let Ok(data) = s.recv().await {
            let reading = FishTankReading {
                distance: data.distance,
                temperature: data.temperature,
                tds: data.tds,
                time: Utc::now(),
            }
            .into_query(&topic);

            if is_debug_mode() {
                debug!("would send {:?}", reading);
            } else if let Err(e) = client.query(&reading).await {
                error!("Failed to write to influxdb: {}", e);
            }
        }
    });

    Ok(())
}

pub fn run(state: &mut State) -> Result<(), EnvironmentError> {
    monitor_float_value::<anavi::Temperature>(
        state,
        "workgroup/3765653003a76f301ad767b4676d7065/air/temperature",
    )?;
    monitor_float_value::<anavi::Humidity>(
        state,
        "workgroup/3765653003a76f301ad767b4676d7065/air/humidity",
    )?;
    monitor_float_value::<anavi::Temperature>(
        state,
        "workgroup/3765653003a76f301ad767b4676d7065/water/temperature",
    )?;
    monitor_fishtank(state, "fishtank/sensors")?;
    Ok(())
}
