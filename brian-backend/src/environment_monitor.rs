use chrono::{DateTime, Utc};
use influxdb::{Client, InfluxDbWriteable};

use robotica_backend::pipes::{Subscriber, Subscription};
use robotica_backend::{get_env, is_debug_mode, spawn, EnvironmentError};
use robotica_common::anavi_thermometer::{self as anavi};
use robotica_common::mqtt::{Json, MqttMessage};
use robotica_common::zwave;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tracing::{debug, error};

use crate::State;

#[derive(Debug, InfluxDbWriteable)]
struct InfluxReadingF64 {
    value: f64,
    time: DateTime<Utc>,
}

impl From<anavi::Temperature> for InfluxReadingF64 {
    fn from(reading: anavi::Temperature) -> Self {
        Self {
            value: reading.temperature,
            time: Utc::now(),
        }
    }
}

impl From<anavi::Humidity> for InfluxReadingF64 {
    fn from(reading: anavi::Humidity) -> Self {
        Self {
            value: reading.humidity,
            time: Utc::now(),
        }
    }
}

impl From<zwave::Data<f64>> for InfluxReadingF64 {
    fn from(reading: zwave::Data<f64>) -> Self {
        Self {
            value: reading.value,
            time: reading.get_datetime().unwrap_or_else(Utc::now),
        }
    }
}

#[derive(Debug, InfluxDbWriteable)]
struct InfluxReadingU8 {
    value: u8,
    time: DateTime<Utc>,
}

impl From<zwave::Data<u8>> for InfluxReadingU8 {
    fn from(reading: zwave::Data<u8>) -> Self {
        Self {
            value: reading.value,
            time: reading.get_datetime().unwrap_or_else(Utc::now),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
struct FishTankData {
    distance: u16,
    temperature: f32,
    tds: f32,
}

#[derive(InfluxDbWriteable)]
struct FishTankReading {
    distance: u16,
    temperature: f32,
    tds: f32,
    time: DateTime<Utc>,
}

pub fn monitor_reading<T, Influx>(state: &mut State, topic: &str) -> Result<(), EnvironmentError>
where
    T: Clone + Send + 'static + Into<Influx> + DeserializeOwned,
    Influx: InfluxDbWriteable + Send,
    Json<T>: TryFrom<MqttMessage>,
    <Json<T> as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
{
    let rx = state
        .subscriptions
        .subscribe_into_stateless::<Json<T>>(topic);
    let topic = topic.to_string();
    let influx_url = get_env("INFLUXDB_URL")?;
    let influx_database = get_env("INFLUXDB_DATABASE")?;

    spawn(async move {
        let client = Client::new(&influx_url, &influx_database);
        let mut s = rx.subscribe().await;

        while let Ok(Json(data)) = s.recv().await {
            let reading: Influx = data.into();
            let query = reading.into_query(&topic);

            if is_debug_mode() {
                debug!("would send {:?}", query);
            } else if let Err(e) = client.query(&query).await {
                error!("Failed to write to influxdb: {}", e);
            }
        }
    });

    Ok(())
}

pub fn monitor_fishtank(state: &mut State, topic: &str) -> Result<(), EnvironmentError> {
    let rx = state
        .subscriptions
        .subscribe_into_stateless::<Json<FishTankData>>(topic);
    let topic = topic.to_string();
    let influx_url = get_env("INFLUXDB_URL")?;
    let influx_database = get_env("INFLUXDB_DATABASE")?;

    spawn(async move {
        let client = Client::new(&influx_url, &influx_database);
        let mut s = rx.subscribe().await;

        while let Ok(Json(data)) = s.recv().await {
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

fn monitor_zwave_switch(state: &mut State, topic_substr: &str) -> Result<(), EnvironmentError> {
    // kwh
    monitor_reading::<zwave::Data<f64>, InfluxReadingF64>(
        state,
        &format!("zwave/{topic_substr}/50/0/value/65537"),
    )?;

    // watts
    monitor_reading::<zwave::Data<f64>, InfluxReadingF64>(
        state,
        &format!("zwave/{topic_substr}/50/0/value/66049"),
    )?;

    // voltage
    monitor_reading::<zwave::Data<f64>, InfluxReadingF64>(
        state,
        &format!("zwave/{topic_substr}/50/0/value/66561"),
    )?;

    // current
    monitor_reading::<zwave::Data<f64>, InfluxReadingF64>(
        state,
        &format!("zwave/{topic_substr}/50/0/value/66817"),
    )?;

    Ok(())
}

pub fn run(state: &mut State) -> Result<(), EnvironmentError> {
    monitor_reading::<anavi::Temperature, InfluxReadingF64>(
        state,
        "workgroup/3765653003a76f301ad767b4676d7065/air/temperature",
    )?;
    monitor_reading::<anavi::Humidity, InfluxReadingF64>(
        state,
        "workgroup/3765653003a76f301ad767b4676d7065/air/humidity",
    )?;
    monitor_reading::<anavi::Temperature, InfluxReadingF64>(
        state,
        "workgroup/3765653003a76f301ad767b4676d7065/water/temperature",
    )?;

    monitor_reading::<zwave::Data<f64>, InfluxReadingF64>(
        state,
        "zwave/Akiras_Bedroom/Akiras_Environment/49/0/Air_temperature",
    )?;

    monitor_reading::<zwave::Data<u8>, InfluxReadingU8>(
        state,
        "zwave/Akiras_Bedroom/Akiras_Environment/49/0/Humidity",
    )?;

    monitor_reading::<zwave::Data<f64>, InfluxReadingF64>(
        state,
        "zwave/Akiras_Bedroom/Akiras_Environment/49/0/Dew_point",
    )?;

    monitor_zwave_switch(state, "Brians_Bedroom/Desk")?;
    monitor_zwave_switch(state, "Kitchen/Fridge")?;
    monitor_zwave_switch(state, "Laundry/Freezer")?;
    monitor_zwave_switch(state, "Workshop/Pump")?;

    monitor_fishtank(state, "fishtank/sensors")?;
    Ok(())
}
