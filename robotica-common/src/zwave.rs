//! Structures for Z-Wave devices over MQTT

use chrono::{DateTime, NaiveDateTime, Utc};

/// Generic Z-Wave Data
#[derive(serde::Deserialize, Clone)]
pub struct Data<T> {
    /// The time the data was received
    pub time: i64,

    /// The value of the data
    pub value: T,
}

impl<T> Data<T> {
    /// Get the datetime of the reading.
    pub fn get_datetime(&self) -> Option<DateTime<Utc>> {
        let naive = NaiveDateTime::from_timestamp_millis(self.time)?;
        Some(DateTime::from_utc(naive, Utc))
    }
}

/// Tests
#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json;

    #[test]
    fn test_zwave() {
        let data = r#"{
            "time": 1684470932558,
            "value": 1.0
        }"#;

        let data: Data<f32> = serde_json::from_str(data).unwrap();
        let datetime = data.get_datetime().unwrap();

        assert_eq!(
            datetime,
            chrono::DateTime::parse_from_rfc3339("2023-05-19T04:35:32.558Z").unwrap()
        );
    }
}
