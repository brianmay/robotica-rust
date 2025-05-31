//! Shelly EM types

use serde::Deserialize;
use serde::Serialize;

#[cfg(feature = "chrono")]
use chrono::{DateTime, Utc};

/// MQTT message sent by the Shelly EM
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Notify {
    /// The source of the message
    pub src: String,

    /// The destination of the message
    pub dst: String,

    /// The parameters of the message
    #[serde(flatten)]
    pub params: Params,
}

/// Parameters of the MQTT message sent by the Shelly EM
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "method", content = "params")]
pub enum Params {
    /// `NotifyStatus` message
    #[serde(rename = "NotifyStatus")]
    NotifyStatus {
        /// The timestamp of the message
        ts: f64,

        /// The status of the message
        #[serde(rename = "em:0")]
        em_0: Option<Em0>,
        // The status of the message
        // #[serde(rename = "emdata:0")]
        // emdata_0: Option<Em0Data>,
    },

    /// `NotifyFullStatus` message
    #[serde(rename = "NotifyFullStatus")]
    NotifyFullStatus {
        /// The timestamp of the message
        ts: f64,
        // TODO: Add the rest of the fields
    },

    /// `NotifyEvent` message
    #[serde(rename = "NotifyEvent")]
    NotifyEvent {
        /// The timestamp of the message
        ts: f64,
        // TODO: Add the rest of the fields
    },
}

impl Params {
    /// Get the datetime of the reading.
    #[cfg(feature = "chrono")]
    #[must_use]
    pub fn get_datetime(&self) -> Option<DateTime<Utc>> {
        let ts = match self {
            Params::NotifyEvent { ts, .. }
            | Params::NotifyStatus { ts, .. }
            | Params::NotifyFullStatus { ts } => *ts,
        };
        #[allow(clippy::cast_possible_truncation)]
        DateTime::from_timestamp_millis((ts * 1000.0) as i64)
    }
}

/// The status of the Shelly EM
///
/// See <https://shelly-api-docs.shelly.cloud/gen2/ComponentsAndServices/EM>
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Em0 {
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

/// The temperature field of `SwitchStatus`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Temperature {
    /// The temperature in degrees Celsius
    pub t_c: f32,

    /// The temperature in degrees Fahrenheit
    pub t_f: f32,
}

/// The status of the Shelly Switch
///
/// See <https://shelly-api-docs.shelly.cloud/gen2/ComponentsAndServices/Switch>
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SwitchStatus {
    /// The id of the message
    pub id: i64,

    /// The source of the last command
    pub source: String,

    /// The state of the switch
    pub output: bool,

    /// The temperature of the switch
    pub temperature: Temperature,
}

/// A command for the Shelly Switch
#[derive(Debug)]
pub enum SwitchCommand {
    /// Causes the status of the corresponding component to be published.
    StatusUpdate,

    /// Turns the switch on, optional `toggle_after` parameter.
    On(Option<u8>),

    /// Turns the switch off, optional `toggle_after` parameter.
    Off(Option<u8>),

    /// Toggles the switch.
    Toggle,
}

impl From<SwitchCommand> for String {
    fn from(val: SwitchCommand) -> Self {
        match val {
            SwitchCommand::StatusUpdate => "status_update".to_string(),
            SwitchCommand::On(toggle_after) => toggle_after.as_ref().map_or_else(
                || "on".to_string(),
                |toggle_after| format!("on,{toggle_after}"),
            ),
            SwitchCommand::Off(toggle_after) => toggle_after.as_ref().map_or_else(
                || "off".to_string(),
                |toggle_after| format!("off,{toggle_after}"),
            ),
            SwitchCommand::Toggle => "toggle".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    #[cfg(feature = "chrono")]
    fn test_shelly_em() {
        let data = r#"{
            "src": "emeter/0",
            "dst": "shelly/shellyem-B8B7F1",
            "method": "NotifyStatus",
            "params": {
                "ts": 1684470932558,
                "em:0": {
                    "id": 0,
                    "a_act_power": 0.0,
                    "a_aprt_power": 0.0,
                    "a_current": 0.0,
                    "a_freq": 0.0,
                    "a_pf": 0.0,
                    "a_voltage": 0.0,
                    "b_act_power": 0.0,
                    "b_aprt_power": 0.0,
                    "b_current": 0.0,
                    "b_freq": 0.0,
                    "b_pf": 0.0,
                    "b_voltage": 0.0,
                    "c_act_power": 0.0,
                    "c_aprt_power": 0.0,
                    "c_current": 0.0,
                    "c_freq": 0.0,
                    "c_pf": 0.0,
                    "c_voltage": 0.0,
                    "n_current": 0.0,
                    "total_act_power": 0.0,
                    "total_aprt_power": 0.0,
                    "total_current": 0.0
                }
            }
        }"#;

        let data: Notify = serde_json::from_str(data).unwrap();

        assert_eq!(data.src, "emeter/0");
        assert_eq!(data.dst, "shelly/shellyem-B8B7F1");

        let Params::NotifyStatus {
            ts,
            em_0: Some(em_0),
        } = data.params
        else {
            panic!();
        };

        assert_abs_diff_eq!(ts, 1_684_470_932_558.0);

        assert_abs_diff_eq!(em_0.id, 0);
        assert_abs_diff_eq!(em_0.a_act_power, 0.0);
        assert_abs_diff_eq!(em_0.a_aprt_power, 0.0);
        assert_abs_diff_eq!(em_0.a_current, 0.0);
    }
}
