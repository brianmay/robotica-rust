use chrono::{DateTime, Utc};
use influxdb::{InfluxDbWriteable, WriteQuery};
use robotica_common::anavi_thermometer::{self as anavi};
use robotica_common::mqtt::{Json, MqttMessage, Parsed};
use robotica_common::{shelly, zwave};
use robotica_tokio::pipes::{Subscriber, Subscription};
use robotica_tokio::services::mqtt;
use robotica_tokio::spawn;
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
    ClimacontrolAc,
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
    ClimacontrolF64,
    ClimacontrolI32,
    ClimacontrolU8,
    ClimacontrolU32,
    ClimacontrolU64,
    ClimacontrolBool,
    ClimacontrolString,
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
            ConfigMetricType::ClimacontrolAc => {
                climacontrol_ac_metrics(&metric.mqtt_topic, &metric.influx_topic)
            }
        }
    }
}

/// Expand a single `climacontrol_ac` config entry into one `RawMetric` per
/// MQTT subtopic published by the Climacontrol heatpump firmware.
///
/// `mqtt_topic` and `influx_topic` are the prefixes supplied by the user
/// (e.g. `climacontrol/HVAC_E07578` and `ac/dining_room`); each subtopic
/// is appended to both.
fn climacontrol_ac_metrics(mqtt_topic: &str, influx_topic: &str) -> Vec<RawMetric> {
    #[allow(clippy::too_many_lines)]
    let entries: &[(&str, RawMetricType)] = &[
        // heatpump — numeric fields
        (
            "heatpump/actual_temperature",
            RawMetricType::ClimacontrolF64,
        ),
        ("heatpump/set_temperature", RawMetricType::ClimacontrolF64),
        ("heatpump/tout", RawMetricType::ClimacontrolF64),
        ("heatpump/tpcns", RawMetricType::ClimacontrolF64),
        ("heatpump/pinp", RawMetricType::ClimacontrolU32),
        ("heatpump/optime", RawMetricType::ClimacontrolU64),
        // heatpump — boolean fields
        ("heatpump/oper", RawMetricType::ClimacontrolBool),
        ("heatpump/defrost", RawMetricType::ClimacontrolBool),
        ("heatpump/filter", RawMetricType::ClimacontrolBool),
        ("heatpump/standby", RawMetricType::ClimacontrolBool),
        ("heatpump/isee", RawMetricType::ClimacontrolBool),
        // heatpump — string tags
        ("heatpump/mode", RawMetricType::ClimacontrolString),
        ("heatpump/power", RawMetricType::ClimacontrolString),
        ("heatpump/fan", RawMetricType::ClimacontrolString),
        ("heatpump/fault_code", RawMetricType::ClimacontrolString),
        // remote thermometer sensor
        ("sensor/thermometer/tact", RawMetricType::ClimacontrolF64),
        ("sensor/thermometer/hact", RawMetricType::ClimacontrolU8),
        ("sensor/thermometer/batt", RawMetricType::ClimacontrolU8),
        ("sensor/thermometer/rssi", RawMetricType::ClimacontrolI32),
        // device link quality and uptime
        ("wifi/rssi", RawMetricType::ClimacontrolI32),
        ("sys/up", RawMetricType::ClimacontrolU64),
    ];

    entries
        .iter()
        .map(|(suffix, ty)| RawMetric {
            mqtt_topic: format!("{mqtt_topic}/{suffix}"),
            influx_topic: format!("{influx_topic}/{suffix}"),
            metric_type: *ty,
        })
        .collect()
}

impl RawMetric {
    pub fn monitor(&self, subscriptions: &mut mqtt::Subscriptions, config: &Config) {
        match self.metric_type {
            RawMetricType::ShellySwitchStatus => {
                monitor_reading::<Json<shelly::SwitchStatus>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ShellyNotify => {
                monitor_reading::<Json<shelly::Notify>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::FishTank => {
                monitor_reading::<Json<FishTankData>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::AnaviTemperature => {
                monitor_reading::<Json<anavi::Temperature>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::AnaviHumidity => {
                monitor_reading::<Json<anavi::Humidity>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ZwaveF64 => {
                monitor_reading::<Json<zwave::Data<f64>>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ZwaveU8 => {
                monitor_reading::<Json<zwave::Data<u8>>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ClimacontrolF64
            | RawMetricType::ClimacontrolI32
            | RawMetricType::ClimacontrolU8
            | RawMetricType::ClimacontrolU32
            | RawMetricType::ClimacontrolU64
            | RawMetricType::ClimacontrolBool
            | RawMetricType::ClimacontrolString => {
                self.monitor_climacontrol(subscriptions, config);
            }
        }
    }

    /// Dispatch the seven `Climacontrol*` raw metric types to a
    /// `monitor_reading` call with the matching plain (non-`Json`) payload type.
    fn monitor_climacontrol(&self, subscriptions: &mut mqtt::Subscriptions, config: &Config) {
        match self.metric_type {
            RawMetricType::ClimacontrolF64 => {
                monitor_reading::<Parsed<f64>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ClimacontrolI32 => {
                monitor_reading::<Parsed<i32>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ClimacontrolU8 => {
                monitor_reading::<Parsed<u8>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ClimacontrolU32 => {
                monitor_reading::<Parsed<u32>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ClimacontrolU64 => {
                monitor_reading::<Parsed<u64>>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ClimacontrolBool => {
                monitor_reading::<bool>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            RawMetricType::ClimacontrolString => {
                monitor_reading::<String>(
                    subscriptions,
                    &self.mqtt_topic,
                    &self.influx_topic,
                    config,
                );
            }
            // Unreachable: caller only invokes this for Climacontrol* types.
            _ => {}
        }
    }
}

trait GetQueries {
    type Error;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error>;
}

/// Generic `InfluxDB` reading for a single numeric/boolean field. The value is
/// stored under the field name `value`.
#[derive(Debug, InfluxDbWriteable)]
struct InfluxFieldReading<T: Into<influxdb::Type>> {
    value: T,
    time: DateTime<Utc>,
}

/// `InfluxDB` reading for a single string value, written as a tag named `value`
/// so it can be indexed and used in `GROUP BY` clauses.
#[derive(Debug, InfluxDbWriteable)]
struct InfluxTagReading {
    #[influxdb(tag)]
    value: String,
    time: DateTime<Utc>,
}

type InfluxFieldReadingError<T> = <InfluxFieldReading<T> as InfluxDbWriteable>::Error;
type InfluxTagReadingError = <InfluxTagReading as InfluxDbWriteable>::Error;

impl GetQueries for Json<anavi::Temperature> {
    type Error = InfluxFieldReadingError<f64>;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        let reading = InfluxFieldReading {
            value: self.0.temperature,
            time: Utc::now(),
        };
        Ok(vec![reading.try_into_query(topic)?])
    }
}

impl GetQueries for Json<anavi::Humidity> {
    type Error = InfluxFieldReadingError<f64>;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        let reading = InfluxFieldReading {
            value: self.0.humidity,
            time: Utc::now(),
        };
        Ok(vec![reading.try_into_query(topic)?])
    }
}

impl GetQueries for Json<zwave::Data<f64>> {
    type Error = InfluxFieldReadingError<f64>;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        let reading = InfluxFieldReading {
            value: self.0.value,
            time: self.0.get_datetime().unwrap_or_else(Utc::now),
        };
        Ok(vec![reading.try_into_query(topic)?])
    }
}

impl GetQueries for Json<zwave::Data<u8>> {
    type Error = InfluxFieldReadingError<u8>;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        let reading = InfluxFieldReading {
            value: self.0.value,
            time: self.0.get_datetime().unwrap_or_else(Utc::now),
        };
        Ok(vec![reading.try_into_query(topic)?])
    }
}

impl GetQueries for Json<shelly::SwitchStatus> {
    type Error = ShellySwitchReadingError;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        let reading = ShellySwitchReading {
            output: self.0.output,
            temperature: self.0.temperature.t_c,
            time: Utc::now(),
        };
        Ok(vec![reading.try_into_query(topic)?])
    }
}

impl GetQueries for Json<FishTankData> {
    type Error = FishTankReadingError;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        let reading = FishTankReading {
            distance: self.0.distance,
            temperature: self.0.temperature,
            tds: self.0.tds,
            time: Utc::now(),
        };
        Ok(vec![reading.try_into_query(topic)?])
    }
}

impl GetQueries for Json<shelly::Notify> {
    type Error = ShellyReadingError;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        let time = self.0.params.get_datetime().unwrap_or_else(Utc::now);
        let topic = |suffix| format!("{topic}/{suffix}");

        if let shelly::Params::NotifyStatus {
            em_0: Some(status), ..
        } = self.0.params
        {
            Ok(vec![
                ShellyReading {
                    time,
                    act_power: status.a_act_power,
                    aprt_power: status.a_aprt_power,
                    current: status.a_current,
                    freq: status.a_freq,
                    pf: status.a_pf,
                    voltage: status.a_voltage,
                }
                .try_into_query(topic("a"))?,
                ShellyReading {
                    time,
                    act_power: status.b_act_power,
                    aprt_power: status.b_aprt_power,
                    current: status.b_current,
                    freq: status.b_freq,
                    pf: status.b_pf,
                    voltage: status.b_voltage,
                }
                .try_into_query(topic("b"))?,
                ShellyReading {
                    time,
                    act_power: status.c_act_power,
                    aprt_power: status.c_aprt_power,
                    current: status.c_current,
                    freq: status.c_freq,
                    pf: status.c_pf,
                    voltage: status.c_voltage,
                }
                .try_into_query(topic("c"))?,
            ])
        } else {
            Ok(vec![])
        }
    }
}

// Plain (non-JSON) MQTT payload impls — used by Climacontrol AC metrics.

impl GetQueries for Parsed<f64> {
    type Error = InfluxFieldReadingError<f64>;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        Ok(vec![InfluxFieldReading {
            value: self.0,
            time: Utc::now(),
        }
        .try_into_query(topic)?])
    }
}

impl GetQueries for Parsed<i32> {
    type Error = InfluxFieldReadingError<i32>;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        Ok(vec![InfluxFieldReading {
            value: self.0,
            time: Utc::now(),
        }
        .try_into_query(topic)?])
    }
}

impl GetQueries for Parsed<u8> {
    type Error = InfluxFieldReadingError<u8>;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        Ok(vec![InfluxFieldReading {
            value: self.0,
            time: Utc::now(),
        }
        .try_into_query(topic)?])
    }
}

impl GetQueries for Parsed<u32> {
    type Error = InfluxFieldReadingError<u32>;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        Ok(vec![InfluxFieldReading {
            value: self.0,
            time: Utc::now(),
        }
        .try_into_query(topic)?])
    }
}

impl GetQueries for Parsed<u64> {
    type Error = InfluxFieldReadingError<u64>;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        Ok(vec![InfluxFieldReading {
            value: self.0,
            time: Utc::now(),
        }
        .try_into_query(topic)?])
    }
}

impl GetQueries for bool {
    type Error = InfluxFieldReadingError<Self>;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        Ok(vec![InfluxFieldReading {
            value: self,
            time: Utc::now(),
        }
        .try_into_query(topic)?])
    }
}

impl GetQueries for String {
    type Error = InfluxTagReadingError;
    fn get_queries(self, topic: &str) -> Result<Vec<WriteQuery>, Self::Error> {
        Ok(vec![InfluxTagReading {
            value: self,
            time: Utc::now(),
        }
        .try_into_query(topic)?])
    }
}

#[derive(Debug, InfluxDbWriteable)]
struct ShellySwitchReading {
    output: bool,
    temperature: f32,
    time: DateTime<Utc>,
}

type ShellySwitchReadingError = <ShellySwitchReading as InfluxDbWriteable>::Error;

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

type FishTankReadingError = <FishTankReading as InfluxDbWriteable>::Error;

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

type ShellyReadingError = <ShellyReading as InfluxDbWriteable>::Error;

fn monitor_reading<T>(
    subscriptions: &mut mqtt::Subscriptions,
    mqtt_topic: &str,
    influx_topic: &str,
    config: &Config,
) where
    T: Clone + Send + 'static + GetQueries + TryFrom<MqttMessage>,
    <T as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
    <T as GetQueries>::Error: Send,
{
    let rx = subscriptions.subscribe_into_stateless::<T>(mqtt_topic);
    let influx_topic = influx_topic.to_string();
    let config = config.clone();

    spawn(async move {
        let client = config.get_client();
        let mut s = rx.subscribe().await;

        while let Ok(data) = s.recv().await {
            if let Ok(queries) = data.get_queries(&influx_topic) {
                for query in queries {
                    tracing::debug!("Writing to influxdb: {:?}", query);
                    if let Err(e) = client.query(&query).await {
                        error!("Failed to write to influxdb: {}", e);
                    }
                }
            } else {
                error!("Failed to create influxdb queries");
            }
        }
    });
}
