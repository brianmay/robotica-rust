use chrono::{DateTime, Utc};
use influxdb::{InfluxDbWriteable, WriteQuery};
use robotica_backend::pipes::{Subscriber, Subscription};
use robotica_backend::spawn;
use robotica_common::anavi_thermometer::{self as anavi};
use robotica_common::mqtt::{Json, MqttMessage};
use robotica_common::{shelly, zwave};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tap::Pipe;
use tracing::error;

use crate::influxdb::Config;
use crate::InitState;

trait GetQueries {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery>;
}

#[derive(Debug, InfluxDbWriteable)]
struct InfluxReadingF64 {
    value: f64,
    time: DateTime<Utc>,
}

impl GetQueries for anavi::Temperature {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        InfluxReadingF64 {
            value: self.temperature,
            time: Utc::now(),
        }
        .pipe(|x| x.into_query(topic))
        .pipe(|x| vec![x])
    }
}

impl GetQueries for anavi::Humidity {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        InfluxReadingF64 {
            value: self.humidity,
            time: Utc::now(),
        }
        .pipe(|x| x.into_query(topic))
        .pipe(|x| vec![x])
    }
}

impl GetQueries for zwave::Data<f64> {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        InfluxReadingF64 {
            value: self.value,
            time: self.get_datetime().unwrap_or_else(Utc::now),
        }
        .pipe(|x| x.into_query(topic))
        .pipe(|x| vec![x])
    }
}

#[derive(Debug, InfluxDbWriteable)]
struct InfluxReadingU8 {
    value: u8,
    time: DateTime<Utc>,
}

impl GetQueries for zwave::Data<u8> {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        InfluxReadingU8 {
            value: self.value,
            time: self.get_datetime().unwrap_or_else(Utc::now),
        }
        .pipe(|x| x.into_query(topic))
        .pipe(|x| vec![x])
    }
}

#[derive(Debug, InfluxDbWriteable)]
struct ShellySwitchReading {
    output: bool,
    temperature: f32,
    time: DateTime<Utc>,
}

impl GetQueries for shelly::SwitchStatus {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        ShellySwitchReading {
            output: self.output,
            temperature: self.temperature.t_c,
            time: Utc::now(),
        }
        .pipe(|x| x.into_query(topic))
        .pipe(|x| vec![x])
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

impl GetQueries for FishTankData {
    fn get_queries(self, topic: &str) -> Vec<WriteQuery> {
        FishTankReading {
            distance: self.distance,
            temperature: self.temperature,
            tds: self.tds,
            time: Utc::now(),
        }
        .pipe(|x| x.into_query(topic))
        .pipe(|x| vec![x])
    }
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
                .into_query(topic("a")),
                ShellyReading {
                    time,
                    act_power: status.b_act_power,
                    aprt_power: status.b_aprt_power,
                    current: status.b_current,
                    freq: status.b_freq,
                    pf: status.b_pf,
                    voltage: status.b_voltage,
                }
                .into_query(topic("b")),
                ShellyReading {
                    time,
                    act_power: status.c_act_power,
                    aprt_power: status.c_aprt_power,
                    current: status.c_current,
                    freq: status.c_freq,
                    pf: status.c_pf,
                    voltage: status.c_voltage,
                }
                .into_query(topic("c")),
            ]
        } else {
            vec![]
        }
    }
}

fn monitor_reading<T>(state: &mut InitState, mqtt_topic: &str, influx_topic: &str, config: &Config)
where
    T: Clone + Send + 'static + GetQueries + DeserializeOwned,
    Json<T>: TryFrom<MqttMessage>,
    <Json<T> as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
{
    let rx = state
        .subscriptions
        .subscribe_into_stateless::<Json<T>>(mqtt_topic);
    let influx_topic = influx_topic.to_string();
    let config = config.clone();

    spawn(async move {
        let client = config.get_client();
        let mut s = rx.subscribe().await;

        while let Ok(Json(data)) = s.recv().await {
            for query in data.get_queries(&influx_topic) {
                tracing::debug!("Writing to influxdb: {:?}", query);
                if let Err(e) = client.query(&query).await {
                    error!("Failed to write to influxdb: {}", e);
                }
            }
        }
    });
}

fn monitor_zwave_switch(state: &mut InitState, topic_substr: &str, config: &Config) {
    // kwh
    monitor_reading::<zwave::Data<f64>>(
        state,
        &format!("zwave/{topic_substr}/50/0/value/65537"),
        &format!("zwave/{topic_substr}/50/0/value/65537"),
        config,
    );

    // watts
    monitor_reading::<zwave::Data<f64>>(
        state,
        &format!("zwave/{topic_substr}/50/0/value/66049"),
        &format!("zwave/{topic_substr}/50/0/value/66049"),
        config,
    );

    // voltage
    monitor_reading::<zwave::Data<f64>>(
        state,
        &format!("zwave/{topic_substr}/50/0/value/66561"),
        &format!("zwave/{topic_substr}/50/0/value/66561"),
        config,
    );

    // current
    monitor_reading::<zwave::Data<f64>>(
        state,
        &format!("zwave/{topic_substr}/50/0/value/66817"),
        &format!("zwave/{topic_substr}/50/0/value/66817"),
        config,
    );
}

pub fn run(state: &mut InitState, config: &Config) {
    monitor_reading::<anavi::Temperature>(
        state,
        "workgroup/3765653003a76f301ad767b4676d7065/air/temperature",
        "workgroup/3765653003a76f301ad767b4676d7065/air/temperature",
        config,
    );
    monitor_reading::<anavi::Humidity>(
        state,
        "workgroup/3765653003a76f301ad767b4676d7065/air/humidity",
        "workgroup/3765653003a76f301ad767b4676d7065/air/humidity",
        config,
    );
    monitor_reading::<anavi::Temperature>(
        state,
        "workgroup/3765653003a76f301ad767b4676d7065/water/temperature",
        "workgroup/3765653003a76f301ad767b4676d7065/water/temperature",
        config,
    );

    monitor_reading::<zwave::Data<f64>>(
        state,
        "zwave/Akiras_Bedroom/Akiras_Environment/49/0/Air_temperature",
        "zwave/Akiras_Bedroom/Akiras_Environment/49/0/Air_temperature",
        config,
    );

    monitor_reading::<zwave::Data<u8>>(
        state,
        "zwave/Akiras_Bedroom/Akiras_Environment/49/0/Humidity",
        "zwave/Akiras_Bedroom/Akiras_Environment/49/0/Humidity",
        config,
    );

    monitor_reading::<zwave::Data<f64>>(
        state,
        "zwave/Akiras_Bedroom/Akiras_Environment/49/0/Dew_point",
        "zwave/Akiras_Bedroom/Akiras_Environment/49/0/Dew_point",
        config,
    );

    monitor_zwave_switch(state, "Brians_Bedroom/Desk", config);
    monitor_zwave_switch(state, "Kitchen/Fridge", config);
    monitor_zwave_switch(state, "Laundry/Freezer", config);
    monitor_zwave_switch(state, "Workshop/Pump", config);

    monitor_reading::<FishTankData>(state, "fishtank/sensors", "fishtank/sensors", config);

    monitor_reading::<shelly::Notify>(
        state,
        "shellypro3em-c8f09e8971ec/events/rpc",
        "shellypro3em-c8f09e8971ec",
        config,
    );

    monitor_reading::<shelly::Notify>(
        state,
        "shellypro3em-ec6260977960/events/rpc",
        "shellypro3em-ec6260977960",
        config,
    );

    monitor_reading::<shelly::SwitchStatus>(state, "hotwater/status/switch:0", "hotwater", config);
}
