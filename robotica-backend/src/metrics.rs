use chrono::{DateTime, Utc};
use influxdb::{InfluxDbWriteable, WriteQuery};
use robotica_common::anavi_thermometer::{self as anavi};
use robotica_common::mqtt::{Json, MqttMessage};
use robotica_common::{shelly, zwave};
use robotica_tokio::pipes::{Subscriber, Subscription};
use robotica_tokio::services::mqtt;
use robotica_tokio::spawn;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tracing::error;

use crate::influxdb::Config;

#[derive(Deserialize, Copy, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ConfigMetricType {
    ShellySwitchStatus,
    ShellyNotify,
    FishTank,
    ZwaveSwitch,
    AnaviTemperature,
    AnaviHumidity,
    ZwaveF64,
    ZwaveU8,
}

#[derive(Deserialize, Debug)]
pub struct ConfigMetric {
    mqtt_topic: String,
    influx_topic: String,
    metric_type: ConfigMetricType,
}

#[derive(Copy, Clone, Debug)]
pub enum RawMetricType {
    ShellySwitchStatus,
    ShellyNotify,
    FishTank,
    AnaviTemperature,
    AnaviHumidity,
    ZwaveF64,
    ZwaveU8,
}

#[derive(Debug)]
pub struct RawMetric {
    mqtt_topic: String,
    influx_topic: String,
    metric_type: RawMetricType,
}

impl From<ConfigMetric> for Vec<RawMetric> {
    fn from(metric: ConfigMetric) -> Self {
        match metric.metric_type {
            ConfigMetricType::ShellySwitchStatus => vec![RawMetric {
                mqtt_topic: metric.mqtt_topic,
                influx_topic: metric.influx_topic,
                metric_type: RawMetricType::ShellySwitchStatus,
            }],
            ConfigMetricType::ShellyNotify => vec![RawMetric {
                mqtt_topic: metric.mqtt_topic,
                influx_topic: metric.influx_topic,
                metric_type: RawMetricType::ShellyNotify,
            }],
            ConfigMetricType::FishTank => vec![RawMetric {
                mqtt_topic: metric.mqtt_topic,
                influx_topic: metric.influx_topic,
                metric_type: RawMetricType::FishTank,
            }],
            ConfigMetricType::ZwaveSwitch => vec![
                RawMetric {
                    mqtt_topic: format!(
                        "{mqtt_topic}/50/0/value/65537",
                        mqtt_topic = metric.mqtt_topic
                    ),
                    influx_topic: format!(
                        "{influx_topic}/50/0/value/65537",
                        influx_topic = metric.influx_topic
                    ),
                    metric_type: RawMetricType::ZwaveF64,
                },
                RawMetric {
                    mqtt_topic: format!(
                        "{mqtt_topic}/50/0/value/66049",
                        mqtt_topic = metric.mqtt_topic
                    ),
                    influx_topic: format!(
                        "{influx_topic}/50/0/value/66049",
                        influx_topic = metric.influx_topic
                    ),
                    metric_type: RawMetricType::ZwaveF64,
                },
                RawMetric {
                    mqtt_topic: format!(
                        "{mqtt_topic}/50/0/value/66561",
                        mqtt_topic = metric.mqtt_topic
                    ),
                    influx_topic: format!(
                        "{influx_topic}/50/0/value/66561",
                        influx_topic = metric.influx_topic
                    ),
                    metric_type: RawMetricType::ZwaveF64,
                },
                RawMetric {
                    mqtt_topic: format!(
                        "{mqtt_topic}/50/0/value/66817",
                        mqtt_topic = metric.mqtt_topic
                    ),
                    influx_topic: format!(
                        "{influx_topic}/50/0/value/66817",
                        influx_topic = metric.influx_topic
                    ),
                    metric_type: RawMetricType::ZwaveF64,
                },
            ],
            ConfigMetricType::AnaviTemperature => vec![RawMetric {
                mqtt_topic: metric.mqtt_topic,
                influx_topic: metric.influx_topic,
                metric_type: RawMetricType::AnaviTemperature,
            }],
            ConfigMetricType::AnaviHumidity => vec![RawMetric {
                mqtt_topic: metric.mqtt_topic,
                influx_topic: metric.influx_topic,
                metric_type: RawMetricType::AnaviHumidity,
            }],
            ConfigMetricType::ZwaveF64 => vec![RawMetric {
                mqtt_topic: metric.mqtt_topic,
                influx_topic: metric.influx_topic,
                metric_type: RawMetricType::ZwaveF64,
            }],
            ConfigMetricType::ZwaveU8 => vec![RawMetric {
                mqtt_topic: metric.mqtt_topic,
                influx_topic: metric.influx_topic,
                metric_type: RawMetricType::ZwaveU8,
            }],
        }
    }
}

impl RawMetric {
    pub fn monitor(&self, subscriptions: &mut mqtt::Subscriptions, config: &Config) {
        match self.metric_type {
            RawMetricType::ShellySwitchStatus => {
                monitor_reading::<shelly::SwitchStatus>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ShellyNotify => {
                monitor_reading::<shelly::Notify>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::FishTank => {
                monitor_reading::<FishTankData>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::AnaviTemperature => {
                monitor_reading::<anavi::Temperature>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::AnaviHumidity => {
                monitor_reading::<anavi::Humidity>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ZwaveF64 => {
                monitor_reading::<zwave::Data<f64>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ZwaveU8 => {
                monitor_reading::<zwave::Data<u8>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
        }
    }
}

trait GetQueries {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery>;
}

#[derive(Debug, InfluxDbWriteable)]
struct InfluxReadingF64 {
    value: f64,
    time: DateTime<Utc>,
}

#[allow(clippy::unwrap_used)]
/// Note: unwrap is used because the influxdb-rs crate's derive macro generates a private
/// error type. See <https://github.com/influxdb-rs/influxdb-rust/issues/188>
impl GetQueries for anavi::Temperature {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        let reading = InfluxReadingF64 {
            value: self.temperature,
            time: Utc::now(),
        };
        vec![reading.try_into_query(topic).unwrap()]
    }
}

#[allow(clippy::unwrap_used)]
/// Note: unwrap is used because the influxdb-rs crate's derive macro generates a private
/// error type. See <https://github.com/influxdb-rs/influxdb-rust/issues/188>
impl GetQueries for anavi::Humidity {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        let reading = InfluxReadingF64 {
            value: self.humidity,
            time: Utc::now(),
        };
        vec![reading.try_into_query(topic).unwrap()]
    }
}

#[allow(clippy::unwrap_used)]
/// Note: unwrap is used because the influxdb-rs crate's derive macro generates a private
/// error type. See <https://github.com/influxdb-rs/influxdb-rust/issues/188>
impl GetQueries for zwave::Data<f64> {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        let reading = InfluxReadingF64 {
            value: self.value,
            time: self.get_datetime().unwrap_or_else(Utc::now),
        };
        vec![reading.try_into_query(topic).unwrap()]
    }
}

#[derive(Debug, InfluxDbWriteable)]
struct InfluxReadingU8 {
    value: u8,
    time: DateTime<Utc>,
}

#[allow(clippy::unwrap_used)]
/// Note: unwrap is used because the influxdb-rs crate's derive macro generates a private
/// error type. See <https://github.com/influxdb-rs/influxdb-rust/issues/188>
impl GetQueries for zwave::Data<u8> {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        let reading = InfluxReadingU8 {
            value: self.value,
            time: self.get_datetime().unwrap_or_else(Utc::now),
        };
        vec![reading.try_into_query(topic).unwrap()]
    }
}

#[allow(clippy::unwrap_used)]
/// Note: unwrap is used because the influxdb-rs crate's derive macro generates a private
/// error type. See <https://github.com/influxdb-rs/influxdb-rust/issues/188>
impl GetQueries for shelly::SwitchStatus {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        let reading = ShellySwitchReading {
            output: self.output,
            temperature: self.temperature.t_c,
            time: Utc::now(),
        };
        vec![reading.try_into_query(topic).unwrap()]
    }
}

#[allow(clippy::unwrap_used)]
/// Note: unwrap is used because the influxdb-rs crate's derive macro generates a private
/// error type. See <https://github.com/influxdb-rs/influxdb-rust/issues/188>
impl GetQueries for FishTankData {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        let reading = FishTankReading {
            distance: self.distance,
            temperature: self.temperature,
            tds: self.tds,
            time: Utc::now(),
        };
        vec![reading.try_into_query(topic).unwrap()]
    }
}

#[derive(Debug, InfluxDbWriteable)]
struct ShellySwitchReading {
    output: bool,
    temperature: f32,
    time: DateTime<Utc>,
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

#[derive(Debug, InfluxDbWriteable)]
struct ShellyReading {
    pub time: DateTime<Utc>,
    pub act_power: f64,
    pub aprt_power: f64,
    pub current: f64,
    pub freq: f64,
    pub pf: f64,
    pub voltage: f64,
}

#[allow(clippy::unwrap_used)]
/// Note: unwrap is used because the influxdb-rs crate's derive macro generates a private
/// error type. See <https://github.com/influxdb-rs/influxdb-rust/issues/188>
impl GetQueries for shelly::Notify {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        let time = self.params.get_datetime().unwrap_or_else(Utc::now);
        let topic = |suffix| format!("{topic}/{suffix}");

        if let shelly::Params::NotifyStatus {
            em_0: Some(status), ..
        } = self.params
        {
            vec![
                ShellyReading {
                    time,
                    act_power: status.a_act_power,
                    aprt_power: status.a_aprt_power,
                    current: status.a_current,
                    freq: status.a_freq,
                    pf: status.a_pf,
                    voltage: status.a_voltage,
                }
                .try_into_query(topic("a"))
                .unwrap(),
                ShellyReading {
                    time,
                    act_power: status.b_act_power,
                    aprt_power: status.b_aprt_power,
                    current: status.b_current,
                    freq: status.b_freq,
                    pf: status.b_pf,
                    voltage: status.b_voltage,
                }
                .try_into_query(topic("b"))
                .unwrap(),
                ShellyReading {
                    time,
                    act_power: status.c_act_power,
                    aprt_power: status.c_aprt_power,
                    current: status.c_current,
                    freq: status.c_freq,
                    pf: status.c_pf,
                    voltage: status.c_voltage,
                }
                .try_into_query(topic("c"))
                .unwrap(),
            ]
        } else {
            vec![]
        }
    }
}

fn monitor_reading<T>(
    subscriptions: &mut mqtt::Subscriptions,
    mqtt_topic: &str,
    influx_topic: &str,
    config: &Config,
) where
    T: Clone + Send + 'static + GetQueries + DeserializeOwned,
    Json<T>: TryFrom<MqttMessage>,
    <Json<T> as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
{
    let rx = subscriptions.subscribe_into_stateless::<Json<T>>(mqtt_topic);
    let influx_topic = influx_topic.to_string();
    let config = config.clone();

    spawn(async move {
        let client = config.get_client();
        let mut s = rx.subscribe().await;

        while let Ok(Json(data)) = s.recv().await {
            let queries = data.get_queries(&influx_topic);
            for query in queries {
                tracing::debug!("Writing to influxdb: {:?}", query);
                if let Err(e) = client.query(&query).await {
                    error!("Failed to write to influxdb: {}", e);
                }
            }
        }
    });
}