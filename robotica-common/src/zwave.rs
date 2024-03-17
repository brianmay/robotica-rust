//! Structures for Z-Wave devices over MQTT

#[cfg(feature = "chrono")]
use chrono::{DateTime, Utc};

/// Generic Z-Wave Data
#[derive(serde::Deserialize, Clone)]
pub struct Data<T> {
    /// The time the data was sent
    pub time: i64,

    /// The value of the data
    pub value: T,
}

impl<T> Data<T> {
    /// Get the datetime of the reading.
    #[cfg(feature = "chrono")]
    pub const fn get_datetime(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp_millis(self.time)
    }
}

/// The status of a Z-Wave device
#[derive(Copy, Clone, serde::Deserialize)]
pub enum Status {
    /// The device is dead
    Dead,

    /// The device is alive
    Alive,

    /// The device is unknown
    Unknown,
}

/// A status message from a Z-Wave device

#[derive(serde::Deserialize)]
pub struct StatusMessage {
    /// The time the status was sent
    pub time: i64,

    /// The status of the device
    pub status: Status,
}

/// Tests
#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    #[test]
    #[cfg(feature = "chrono")]
    fn test_zwave() {
        let data = r#"{
            "time": 1684470932558,
            "value": 1.0
        }"#;

        let data: super::Data<f32> = serde_json::from_str(data).unwrap();
        let datetime = data.get_datetime().unwrap();

        assert_eq!(
            datetime,
            chrono::DateTime::parse_from_rfc3339("2023-05-19T04:35:32.558Z").unwrap()
        );
    }
}
