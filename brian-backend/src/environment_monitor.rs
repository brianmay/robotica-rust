use chrono::{DateTime, Utc};
use influxdb::{Client, InfluxDbWriteable};

use robotica_backend::pipes::{Subscriber, Subscription};
use robotica_backend::{is_debug_mode, spawn};
use robotica_common::anavi_thermometer::{self as anavi};
use robotica_common::mqtt::{Json, MqttMessage};
use robotica_common::zwave;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tracing::{debug, error};

use crate::State;

pub struct Config {
    pub influxdb_url: String,
    pub influxdb_database: String,
}

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

pub fn monitor_reading<T, Influx>(state: &mut State, topic: &str, config: &Config)
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
    let influxdb_url = config.influxdb_url.clone();
    let influxdb_database = config.influxdb_database.clone();

    spawn(async move {
        let client = Client::new(&influxdb_url, &influxdb_database);
        let mut s = rx.subscribe().await;

        while let Ok(Json(data)) = s.recv().await {
            let reading: Influx = data.into();
            let query = reading.into_query(&topic);

            if is_debug_mode() {
                // debug!("would send {:?}", query);
            } else if let Err(e) = client.query(&query).await {
                error!("Failed to write to influxdb: {}", e);
            }
        }
    });
}

pub fn monitor_fishtank(state: &mut State, topic: &str, config: &Config) {
    let rx = state
        .subscriptions
        .subscribe_into_stateless::<Json<FishTankData>>(topic);
    let topic = topic.to_string();
    let influxdb_url = config.influxdb_url.clone();
    let influxdb_database = config.influxdb_database.clone();

    spawn(async move {
        let client = Client::new(&influxdb_url, &influxdb_database);
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
}

fn monitor_zwave_switch(state: &mut State, topic_substr: &str, config: &Config) {
    // kwh
    monitor_reading::<zwave::Data<f64>, InfluxReadingF64>(
        state,
        &format!("zwave/{topic_substr}/50/0/value/65537"),
        config,
    );

    // watts
    monitor_reading::<zwave::Data<f64>, InfluxReadingF64>(
        state,
        &format!("zwave/{topic_substr}/50/0/value/66049"),
        config,
    );

    // voltage
    monitor_reading::<zwave::Data<f64>, InfluxReadingF64>(
        state,
        &format!("zwave/{topic_substr}/50/0/value/66561"),
        config,
    );

    // current
    monitor_reading::<zwave::Data<f64>, InfluxReadingF64>(
        state,
        &format!("zwave/{topic_substr}/50/0/value/66817"),
        config,
    );
}

pub fn run(state: &mut State, config: &Config) {
    monitor_reading::<anavi::Temperature, InfluxReadingF64>(
        state,
        "workgroup/3765653003a76f301ad767b4676d7065/air/temperature",
        config,
    );
    monitor_reading::<anavi::Humidity, InfluxReadingF64>(
        state,
        "workgroup/3765653003a76f301ad767b4676d7065/air/humidity",
        config,
    );
    monitor_reading::<anavi::Temperature, InfluxReadingF64>(
        state,
        "workgroup/3765653003a76f301ad767b4676d7065/water/temperature",
        config,
    );

    monitor_reading::<zwave::Data<f64>, InfluxReadingF64>(
        state,
        "zwave/Akiras_Bedroom/Akiras_Environment/49/0/Air_temperature",
        config,
    );

    monitor_reading::<zwave::Data<u8>, InfluxReadingU8>(
        state,
        "zwave/Akiras_Bedroom/Akiras_Environment/49/0/Humidity",
        config,
    );

    monitor_reading::<zwave::Data<f64>, InfluxReadingF64>(
        state,
        "zwave/Akiras_Bedroom/Akiras_Environment/49/0/Dew_point",
        config,
    );

    monitor_zwave_switch(state, "Brians_Bedroom/Desk", config);
    monitor_zwave_switch(state, "Kitchen/Fridge", config);
    monitor_zwave_switch(state, "Laundry/Freezer", config);
    monitor_zwave_switch(state, "Workshop/Pump", config);

    monitor_fishtank(state, "fishtank/sensors", config);
}
