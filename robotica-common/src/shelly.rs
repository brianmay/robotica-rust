//! Shelly EM types

use serde::Deserialize;
use serde::Serialize;

#[cfg(feature = "chrono")]
use chrono::{DateTime, NaiveDateTime, Utc};

/// MQTT message sent by the Shelly EM
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotifyStatus {
    /// The source of the message
    pub src: String,

    /// The destination of the message
    pub dst: String,

    /// The method of the message
    ///
    /// Should be "NotifyStatus".
    pub method: String,

    /// The parameters of the message
    pub params: Params,
}

/// Parameters of the MQTT message sent by the Shelly EM
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Params {
    /// The timestamp of the message
    pub ts: f64,

    /// The status of the message
    #[serde(rename = "em:0")]
    pub em_0: Em0,
}

impl Params {
    /// Get the datetime of the reading.
    #[cfg(feature = "chrono")]
    #[must_use]
    pub fn get_datetime(&self) -> Option<DateTime<Utc>> {
        #[allow(clippy::cast_possible_truncation)]
        let naive = NaiveDateTime::from_timestamp_millis((self.ts * 1000.0) as i64)?;
        Some(DateTime::from_naive_utc_and_offset(naive, Utc))
    }
}

/// The status of the Shelly EM
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Em0 {
    /// The id of the message
    pub id: i64,

    /// The Active Power for Phase A
    pub a_act_power: f64,

    /// The Apparent Power for Phase A
    pub a_aprt_power: f64,

    /// The Current for Phase A
    pub a_current: f64,

    /// The Frequency for Phase A
    pub a_freq: f64,

    /// The Power Factor for Phase A
    pub a_pf: f64,

    /// The Voltage for Phase A
    pub a_voltage: f64,

    /// The Active Power for Phase B
    pub b_act_power: f64,

    /// The Apparent Power for Phase B
    pub b_aprt_power: f64,

    /// The Current for Phase B
    pub b_current: f64,

    /// The Frequency for Phase B
    pub b_freq: f64,

    /// The Power Factor for Phase B
    pub b_pf: f64,

    /// The Voltage for Phase B
    pub b_voltage: f64,

    /// The Active Power for Phase C
    pub c_act_power: f64,

    /// The Apparent Power for Phase C
    pub c_aprt_power: f64,

    /// The Current for Phase C
    pub c_current: f64,

    /// The Frequency for Phase C
    pub c_freq: f64,

    /// The Power Factor for Phase C
    pub c_pf: f64,

    /// The Voltage for Phase C
    pub c_voltage: f64,

    /// No idea what this is
    pub n_current: Option<f64>,

    /// The Total Active Power
    pub total_act_power: f64,

    /// The Total Apparent Power
    pub total_aprt_power: f64,

    /// The Total Current
    pub total_current: f64,
}
