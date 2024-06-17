//! Serializable datetime types.

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeDelta, TimeZone, Utc};
use std::time::Duration;
use thiserror::Error;

/// A Date.
pub type Date = chrono::NaiveDate;

/// A Time.
pub type Time = chrono::NaiveTime;

/// A Date and a Time for a given Timezone.
pub type DateTime<Tz> = chrono::DateTime<Tz>;

/// A day of the week.
pub type Weekday = chrono::Weekday;

/// Get the number of days from CE of the date.
///
/// Kludge: This function returns values consistent with Elxir's `to_iso_days/1` function.
#[must_use]
pub fn num_days_from_ce(date: &Date) -> i32 {
    date.num_days_from_ce() + 365
}

/// Convert a weekday to a string.
#[must_use]
pub fn week_day_to_string(weekday: Weekday) -> String {
    match weekday {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
    }
    .to_string()
}

/// An error that can occur when translating a `DateTime`.
#[derive(Error, Debug)]
pub enum DateTimeError<T: TimeZone> {
    /// There are no options converting this date time to a local date time.
    #[error("Can't convert DateTime {0} to local timezone")]
    CantConvertDateTime(NaiveDateTime),

    /// There are multiple options converting this date time to a local date time.
    #[error("DateTime {0} is ambiguous when converting to local timezone")]
    AmbiguousDateTime(NaiveDateTime, DateTime<T>, DateTime<T>),
}

/// Get the current time in UTC timezone.
#[must_use]
pub fn utc_now() -> DateTime<Utc> {
    Utc::now()
}

/// Convert a date time to a local string.
#[must_use]
pub fn datetime_to_string(dt: &DateTime<Utc>) -> String {
    let local = dt.with_timezone(&Local);
    local.format("%Y-%m-%d %H:%M:%S %z").to_string()
}

// /// Create a new Duration.
// #[must_use]
// pub fn duration_from_hms(hours: u64, minutes: u64, seconds: u64) -> std::time::Duration {
//     std::time::Duration::from_secs((hours * 60 + minutes) * 60 + seconds)
// }

const fn div_rem_u64(a: u64, b: u64) -> (u64, u64) {
    (a / b, a % b)
}

/// Serde serialization deserialization for a duration.
pub mod with_duration {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Deserialize a duration.
    ///
    /// # Errors
    ///
    /// If the duration is invalid.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<super::Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        let d = super::duration::from_str(&s)
            .map_err(|_| serde::de::Error::custom(format!("Invalid duration {s}")))?;
        Ok(d)
    }

    /// Serialize a duration.
    ///
    /// # Errors
    ///
    /// If the duration is invalid.
    pub fn serialize<S>(duration: &super::Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let result = super::duration::to_string(duration);
        serializer.serialize_str(&result)
    }
}

/// Serde serialization deserialization for a duration.
pub mod with_time_delta {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Deserialize a duration.
    ///
    /// # Errors
    ///
    /// If the duration is invalid.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<super::TimeDelta, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        let d = super::time_delta::from_str(&s)
            .map_err(|_| serde::de::Error::custom(format!("Invalid time delta {s}")))?;
        Ok(d)
    }

    /// Serialize a duration.
    ///
    /// # Errors
    ///
    /// If the duration is negative.
    pub fn serialize<S>(duration: &super::TimeDelta, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let result = super::time_delta::to_string(duration);
        serializer.serialize_str(&result)
    }
}

/// Serde serialization deserialization for a option duration.
pub mod with_option_duration {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct OptionDurationWrapper(#[serde(with = "super::with_duration")] super::Duration);

    /// Deserialize a duration.
    ///
    /// # Errors
    ///
    /// If the duration is invalid.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<super::Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<OptionDurationWrapper>::deserialize(deserializer).map(
            |opt_wrapped: Option<OptionDurationWrapper>| {
                opt_wrapped.map(|wrapped: OptionDurationWrapper| wrapped.0)
            },
        )
    }

    /// Serialize a duration.
    ///
    /// # Errors
    ///
    /// If the duration is negative.
    pub fn serialize<S>(
        duration: &Option<super::Duration>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Option::<OptionDurationWrapper>::serialize(&duration.map(OptionDurationWrapper), serializer)
    }
}

/// Serde serialization deserialization for a option duration.
pub mod with_option_time_delta {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct OptionTimeDeltaWrapper(#[serde(with = "super::with_time_delta")] super::TimeDelta);

    /// Deserialize a duration.
    ///
    /// # Errors
    ///
    /// If the duration is invalid.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<super::TimeDelta>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<OptionTimeDeltaWrapper>::deserialize(deserializer).map(
            |opt_wrapped: Option<OptionTimeDeltaWrapper>| {
                opt_wrapped.map(|wrapped: OptionTimeDeltaWrapper| wrapped.0)
            },
        )
    }

    /// Serialize a duration.
    ///
    /// # Errors
    ///
    /// If the duration is negative.
    pub fn serialize<S>(
        duration: &Option<super::TimeDelta>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Option::<OptionTimeDeltaWrapper>::serialize(
            &duration.map(OptionTimeDeltaWrapper),
            serializer,
        )
    }
}

/// Convert a Date and a Time to a Utc `DateTime`.
///
/// # Errors
///
/// If the date and time cannot be converted to a local date time.
pub fn convert_date_time_to_utc<T: TimeZone>(
    date: Date,
    time: Time,
    timezone: &T,
) -> Result<DateTime<chrono::Utc>, DateTimeError<chrono::Utc>> {
    let datetime = chrono::NaiveDateTime::new(date, time);
    let datetime = match timezone.from_local_datetime(&datetime) {
        chrono::LocalResult::None => Err(DateTimeError::CantConvertDateTime(datetime)),
        chrono::LocalResult::Single(datetime) => Ok(datetime.with_timezone(&chrono::Utc)),
        chrono::LocalResult::Ambiguous(x, y) => Err(DateTimeError::AmbiguousDateTime(
            datetime,
            x.with_timezone(&chrono::Utc),
            y.with_timezone(&chrono::Utc),
        )),
    }?;
    Ok(datetime)
}

/// Convert a Date and a Time to a Utc `DateTime` but do not fail
///
/// If the date and time cannot be converted try to pick a sensible default.
pub fn convert_date_time_to_utc_or_default<T: TimeZone>(
    date: Date,
    time: Time,
    timezone: &T,
) -> DateTime<chrono::Utc> {
    let convert = |time| match convert_date_time_to_utc(date, time, timezone) {
        Ok(datetime) => Some(datetime),
        Err(DateTimeError::AmbiguousDateTime(_, x, _)) => Some(x),
        Err(DateTimeError::CantConvertDateTime(_)) => None,
    };

    convert(time)
        .or_else(|| convert(time - duration::hours(1)))
        .or_else(|| convert(time + duration::hours(1)))
        .unwrap_or_else(|| {
            let datetime = chrono::NaiveDateTime::new(date, time);
            chrono::Utc
                .from_utc_datetime(&datetime)
                .with_timezone(&chrono::Utc)
        })
}

/// `Duration` helpers
pub mod duration {
    use std::time::Duration;
    use thiserror::Error;

    /// Create a new Duration from days.
    #[must_use]
    pub const fn days(u32: u64) -> Duration {
        Duration::from_secs(u32 * 24 * 3600)
    }
    /// Create a new Duration from hours.
    #[must_use]
    pub const fn hours(u32: u64) -> Duration {
        Duration::from_secs(u32 * 3600)
    }

    /// Create a new Duration from minutes.
    #[must_use]
    pub const fn minutes(u32: u64) -> Duration {
        Duration::from_secs(u32 * 60)
    }

    /// Create a new Duration from seconds.
    #[must_use]
    pub const fn seconds(u32: u64) -> Duration {
        Duration::from_secs(u32)
    }

    /// An error that can occur when creating a `Duration`.
    #[derive(Error, Debug)]
    pub enum HmsError {
        /// Minutes overflow
        #[error("Minutes overflow")]
        MinutesOverflow,

        /// Seconds overflow
        #[error("Seconds overflow")]
        SecondsOverflow,

        /// Total seconds overflow
        #[error("Total seconds overflow")]
        TotalSecondsOverflow,
    }

    /// Create a new Duration from hours, minutes and seconds.
    ///
    /// # Errors
    ///
    /// If the hours, minutes or seconds are out of range.
    pub fn try_hms(hours: u64, minutes: u64, seconds: u64) -> Result<Duration, HmsError> {
        if minutes > 59 {
            return Err(HmsError::MinutesOverflow);
        }
        if seconds > 59 {
            return Err(HmsError::SecondsOverflow);
        }

        hours
            .checked_mul(3600)
            .and_then(|x| x.checked_add(minutes * 60))
            .and_then(|x| x.checked_add(seconds))
            .map(Duration::from_secs)
            .ok_or(HmsError::TotalSecondsOverflow)
    }

    /// Get the hours, minutes and seconds of a duration.
    #[must_use]
    pub const fn hms(duration: &Duration) -> (u64, u64, u64) {
        let secs = duration.as_secs();
        let (minutes, secs) = super::div_rem_u64(secs, 60);
        let (hours, minutes) = super::div_rem_u64(minutes, 60);
        (hours, minutes, secs)
    }

    /// Turn a duration into a string.
    #[must_use]
    pub fn to_string(duration: &Duration) -> String {
        let (hours, minutes, seconds) = hms(duration);
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }

    /// An error that can occur when parsing a `Duration`.
    #[derive(Error, Debug)]

    pub enum DurationParseError {
        /// Invalid duration
        #[error("Invalid duration")]
        InvalidDuration,
    }

    /// Turn a string into a `Duration`
    ///
    /// # Errors
    ///
    /// If the string is not a valid `Duration`.
    pub fn from_str(s: &str) -> Result<Duration, DurationParseError> {
        let splits = s.split(':').collect::<Vec<&str>>();

        if s.len() < 2 {
            return Err(DurationParseError::InvalidDuration);
        }

        let hours = splits[0]
            .parse::<u64>()
            .map_err(|_| DurationParseError::InvalidDuration)?;
        let minutes = splits[1]
            .parse::<u64>()
            .map_err(|_| DurationParseError::InvalidDuration)?;

        if splits.len() == 2 {
            try_hms(hours, minutes, 0).map_err(|_| DurationParseError::InvalidDuration)
        } else if splits.len() == 3 {
            let seconds = splits[2]
                .parse::<u64>()
                .map_err(|_| DurationParseError::InvalidDuration)?;
            try_hms(hours, minutes, seconds).map_err(|_| DurationParseError::InvalidDuration)
        } else {
            Err(DurationParseError::InvalidDuration)
        }
    }
}

/// `TimeDelta` helpers
pub mod time_delta {
    use chrono::TimeDelta;
    use thiserror::Error;

    /// An error that can occur when creating a `Duration`.
    #[derive(Error, Debug)]
    pub enum HmsError {
        /// Minutes overflow
        #[error("Minutes overflow")]
        MinutesOverflow,

        /// Seconds overflow
        #[error("Seconds overflow")]
        SecondsOverflow,

        /// Total seconds overflow
        #[error("Total seconds overflow")]
        TotalSecondsOverflow,
    }

    /// Create a new Duration from hours, minutes and seconds.
    ///
    /// # Errors
    ///
    /// If the hours, minutes or seconds are out of range.
    pub fn try_hms(
        positive: bool,
        hours: u64,
        minutes: u64,
        seconds: u64,
    ) -> Result<TimeDelta, HmsError> {
        if minutes > 59 {
            return Err(HmsError::MinutesOverflow);
        }
        if seconds > 59 {
            return Err(HmsError::SecondsOverflow);
        }
        hours
            .checked_mul(3600)
            .and_then(|x| x.checked_add(minutes * 60))
            .and_then(|x| x.checked_add(seconds))
            .and_then::<i64, _>(|x| x.try_into().ok())
            .and_then(|x| x.checked_mul(if positive { 1 } else { -1 }))
            .and_then(TimeDelta::try_seconds)
            .ok_or(HmsError::TotalSecondsOverflow)
    }

    /// Get the hours, minutes and seconds of a duration.
    #[must_use]
    pub const fn hms(duration: &TimeDelta) -> (bool, u64, u64, u64) {
        let secs = duration.num_seconds();
        if secs < 0 {
            #[allow(clippy::cast_sign_loss)]
            let secs = -secs as u64;
            let (minutes, secs) = super::div_rem_u64(secs, 60);
            let (hours, minutes) = super::div_rem_u64(minutes, 60);
            (false, hours, minutes, secs)
        } else {
            #[allow(clippy::cast_sign_loss)]
            let secs = secs as u64;
            let (minutes, secs) = super::div_rem_u64(secs, 60);
            let (hours, minutes) = super::div_rem_u64(minutes, 60);
            (true, hours, minutes, secs)
        }
    }

    /// Turn a duration into a string.
    #[must_use]
    pub fn to_string(duration: &TimeDelta) -> String {
        let (neg, hours, minutes, seconds) = hms(duration);
        format!(
            "{sign}{hours:02}:{minutes:02}:{seconds:02}",
            sign = if neg { "" } else { "-" }
        )
    }

    /// An error that can occur when parsing a `TimeDelta`.
    #[derive(Error, Debug)]
    pub enum TimeDeltaParseError {
        /// Invalid duration
        #[error("Invalid time delta")]
        InvalidTimeDelta,
    }

    /// Turn a string into a `TimeDelta`.
    ///
    /// # Errors
    ///
    /// If the string is not a valid `TimeDelta`.
    pub fn from_str(s: &str) -> Result<TimeDelta, TimeDeltaParseError> {
        let (s, positive) = s
            .strip_prefix('-')
            .map_or((s, true), |stripped| (stripped, false));

        let splits = s.split(':').collect::<Vec<&str>>();

        if s.len() < 2 {
            return Err(TimeDeltaParseError::InvalidTimeDelta);
        }

        let hours = splits[0]
            .parse::<u64>()
            .map_err(|_| TimeDeltaParseError::InvalidTimeDelta)?;
        let minutes = splits[1]
            .parse::<u64>()
            .map_err(|_| TimeDeltaParseError::InvalidTimeDelta)?;

        let seconds = if splits.len() == 2 {
            0
        } else {
            splits[2]
                .parse::<u64>()
                .map_err(|_| TimeDeltaParseError::InvalidTimeDelta)?
        };

        try_hms(positive, hours, minutes, seconds)
            .map_err(|_| TimeDeltaParseError::InvalidTimeDelta)
    }
}

/// An iterator over a range of `NaiveDate`s.
pub struct NaiveDateIter {
    start: NaiveDate,
    end: NaiveDate,
}

impl NaiveDateIter {
    /// Create a new `NaiveDateIter`.
    #[must_use]
    pub const fn new(start: NaiveDate, end: NaiveDate) -> NaiveDateIter {
        NaiveDateIter { start, end }
    }
}

impl Iterator for NaiveDateIter {
    type Item = NaiveDate;
    fn next(&mut self) -> Option<NaiveDate> {
        if self.start.gt(&self.end) {
            None
        } else {
            let res = self.start;
            if let Some(new_start) = self.start.succ_opt() {
                self.start = new_start;
                Some(res)
            } else {
                None
            }
        }
    }
}

/// Macro to create a `Duration` from an integer and panic if out of range
///
/// Note: This macro only intended for static values where the values are known not to overflow.
#[macro_export]
macro_rules! unsafe_duration {
    (days: $days:expr) => {
        $crate::Duration::from_secs($days * 24 * 3600)
    };
    (hours: $hours:expr) => {
        $crate::Duration::from_secs($hours * 3600)
    };
    (minutes: $minutes:expr) => {
        $crate::Duration::from_secs($minutes * 60)
    };
    (seconds: $seconds:expr) => {
        match $crate::Duration::from_secs($seconds)
    };
}

/// Macro to create a `TimeDelta` from an integer and panic if out of range
///
/// Note: This macro only intended for static values where the values are known not to overflow.
#[macro_export]
macro_rules! unsafe_time_delta {
    (days: $days:expr) => {
        match $crate::TimeDelta::try_days($days) {
            Some(duration) => duration,
            None => panic!("days is invalid"),
        }
    };
    (hours: $hours:expr) => {
        match $crate::TimeDelta::try_hours($hours) {
            Some(duration) => duration,
            None => panic!("hours is invalid"),
        }
    };
    (minutes: $minutes:expr) => {
        match $crate::TimeDelta::try_minutes($minutes) {
            Some(duration) => duration,
            None => panic!("minutes is invalid"),
        }
    };
    (seconds: $seconds:expr) => {
        match $crate::TimeDelta::try_seconds($seconds) {
            Some(duration) => duration,
            None => panic!("seconds is invalid"),
        }
    };
}

/// Macro to create a `NaiveDate` from a year, month and day and panic if invalid
///
/// Note: This macro only intended for static values where the values are known not to overflow.
#[macro_export]
macro_rules! unsafe_naive_time_hms {
    ($hours:expr, $minutes:expr, $seconds:expr) => {
        match $crate::NaiveTime::from_hms_opt($hours, $minutes, $seconds) {
            Some(time) => time,
            None => panic!("Invalid time"),
        }
    };
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::str::FromStr;

    use chrono_tz::Tz;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[test]
    fn test_convert_date_time_to_utc() {
        let timezone: Tz = "Australia/Melbourne".parse().unwrap();

        let test = |date, time, expected: Option<&str>| {
            let date = Date::from_str(date).unwrap();
            let time = Time::from_str(time).unwrap();
            let expected: Option<DateTime<Utc>> = expected.map(|x| x.parse().unwrap());

            let datetime = convert_date_time_to_utc(date, time, &timezone).ok();
            assert_eq!(datetime, expected, "date: {date}, time: {time}");
        };

        // At 3am clocks go back to 2am meaning times are ambiguous.
        test("2022-04-03", "01:30:00", Some("2022-04-02T14:30:00Z"));
        test("2022-04-03", "02:30:00", None);
        test("2022-04-03", "03:30:00", Some("2022-04-02T17:30:00Z"));

        // At 2am clocks go forward to 3am meaning times do not exist.
        test("2022-10-02", "01:30:00", Some("2022-10-01T15:30:00Z"));
        test("2022-10-02", "02:30:00", None);
        test("2022-10-02", "03:30:00", Some("2022-10-01T16:30:00Z"));
    }

    #[test]
    fn test_convert_date_time_to_utc_or_default() {
        let timezone: Tz = "Australia/Melbourne".parse().unwrap();

        let test = |date, time, expected: &str| {
            let date = Date::from_str(date).unwrap();
            let time = Time::from_str(time).unwrap();
            let expected: DateTime<Utc> = expected.parse().unwrap();

            let datetime = convert_date_time_to_utc_or_default(date, time, &timezone);
            assert_eq!(datetime, expected, "date: {date}, time: {time}");
        };

        // At 3am clocks go back to 2am meaning times are ambiguous.
        test("2022-04-03", "01:30:00", "2022-04-02T14:30:00Z");
        test("2022-04-03", "02:30:00", "2022-04-02T15:30:00Z");
        test("2022-04-03", "03:30:00", "2022-04-02T17:30:00Z");

        // At 2am clocks go forward to 3am meaning times do not exist.
        test("2022-10-02", "01:30:00", "2022-10-01T15:30:00Z");
        test("2022-10-02", "02:30:00", "2022-10-01T15:30:00Z");
        test("2022-10-02", "03:30:00", "2022-10-01T16:30:00Z");
    }

    #[test]
    fn test_duration_try_hms() {
        let duration = duration::try_hms(1, 2, 3).unwrap();
        assert_eq!(duration.as_secs(), (60 + 2) * 60 + 3);

        let duration = duration::try_hms(1, 60, 3);
        assert!(matches!(duration, Err(duration::HmsError::MinutesOverflow)));

        let duration = duration::try_hms(1, 2, 60);
        assert!(matches!(duration, Err(duration::HmsError::SecondsOverflow)));

        let duration = duration::try_hms(u64::MAX, 2, 3);
        assert!(matches!(
            duration,
            Err(duration::HmsError::TotalSecondsOverflow)
        ));
    }

    #[test]
    fn test_duration_hms() {
        let duration = Duration::from_secs((60 + 2) * 60 + 3);
        assert_eq!(duration::hms(&duration), (1, 2, 3));
    }

    #[test]
    fn test_duration_to_string() {
        let duration = Duration::from_secs((60 + 2) * 60 + 3);
        assert_eq!(duration::to_string(&duration), "01:02:03");
    }

    #[test]
    fn test_duration_from_str() {
        let duration = duration::from_str("1:2").unwrap();
        assert_eq!(duration.as_secs(), (60 + 2) * 60);

        let duration = duration::from_str("1:2:3").unwrap();
        assert_eq!(duration.as_secs(), (60 + 2) * 60 + 3);

        let duration = duration::from_str("1:2:3:4");
        assert!(matches!(
            duration,
            Err(duration::DurationParseError::InvalidDuration)
        ));

        let duration = duration::from_str("1");
        assert!(matches!(
            duration,
            Err(duration::DurationParseError::InvalidDuration)
        ));

        let duration = duration::from_str("1:a:3:4");
        assert!(matches!(
            duration,
            Err(duration::DurationParseError::InvalidDuration)
        ));
    }

    #[derive(Serialize, Deserialize)]
    struct DurationWrapper {
        #[serde(with = "super::with_duration")]
        duration: Duration,
    }

    #[test]
    fn test_duration_serialize() {
        let duration = DurationWrapper {
            duration: duration::from_str("1:2:3").unwrap(),
        };
        let json = serde_json::to_string(&duration).unwrap();
        assert_eq!(json, "{\"duration\":\"01:02:03\"}");
    }

    #[test]
    fn test_duration_deserialize() {
        let json = "{\"duration\":\"01:02:03\"}";
        let DurationWrapper { duration }: DurationWrapper = serde_json::from_str(json).unwrap();
        assert_eq!(duration.as_secs(), (60 + 2) * 60 + 3);
    }

    #[test]
    fn test_time_delta_try_hms() {
        let duration = time_delta::try_hms(true, 1, 2, 3).unwrap();
        assert_eq!(duration.num_seconds(), (60 + 2) * 60 + 3);

        let duration = time_delta::try_hms(false, 1, 2, 3).unwrap();
        assert_eq!(duration.num_seconds(), -((60 + 2) * 60 + 3));

        let duration = time_delta::try_hms(true, 1, 60, 3);
        assert!(matches!(
            duration,
            Err(time_delta::HmsError::MinutesOverflow)
        ));

        let duration = time_delta::try_hms(true, 1, 2, 60);
        assert!(matches!(
            duration,
            Err(time_delta::HmsError::SecondsOverflow)
        ));

        let duration = time_delta::try_hms(true, u64::MAX, 2, 3);
        assert!(matches!(
            duration,
            Err(time_delta::HmsError::TotalSecondsOverflow)
        ));
    }

    #[test]
    fn test_time_delta_hms() {
        let duration = time_delta::from_str("1:2:3").unwrap();
        assert_eq!(time_delta::hms(&duration), (true, 1, 2, 3));

        let duration = time_delta::from_str("-1:2:3").unwrap();
        assert_eq!(time_delta::hms(&duration), (false, 1, 2, 3));
    }

    #[test]
    fn test_time_delta_to_string() {
        let duration = time_delta::from_str("1:2:3").unwrap();
        assert_eq!(time_delta::to_string(&duration), "01:02:03");

        let duration = time_delta::from_str("-1:2:3").unwrap();
        assert_eq!(time_delta::to_string(&duration), "-01:02:03");
    }

    #[derive(Serialize, Deserialize)]
    struct TimeDeltaWrapper {
        #[serde(with = "super::with_time_delta")]
        duration: TimeDelta,
    }

    #[test]
    fn test_time_delta_serialize() {
        let duration = TimeDeltaWrapper {
            duration: time_delta::from_str("1:2:3").unwrap(),
        };
        let json = serde_json::to_string(&duration).unwrap();
        assert_eq!(json, "{\"duration\":\"01:02:03\"}");
    }

    #[test]
    fn test_time_delta_deserialize() {
        let json = "{\"duration\": \"01:02:03\"}";
        let TimeDeltaWrapper { duration } = serde_json::from_str(json).unwrap();
        assert_eq!(duration.num_seconds(), (60 + 2) * 60 + 3);
        assert_eq!(duration.num_minutes(), 60 + 2);
        assert_eq!(duration.num_hours(), 1);
    }

    #[test]
    fn test_negative_time_delta_serialize() {
        let duration = TimeDeltaWrapper {
            duration: time_delta::from_str("-1:2:3").unwrap(),
        };
        let json = serde_json::to_string(&duration).unwrap();
        assert_eq!(json, "{\"duration\":\"-01:02:03\"}");
    }

    #[test]
    fn test_negative_time_delta_deserialize() {
        let json = "{\"duration\": \"-01:02:03\"}";
        let TimeDeltaWrapper { duration } = serde_json::from_str(json).unwrap();
        assert_eq!(duration.num_seconds(), -((60 + 2) * 60 + 3));
        assert_eq!(duration.num_minutes(), -(60 + 2));
        assert_eq!(duration.num_hours(), -1);
    }

    #[test]
    fn test_invalid_negative_time_delta_deserialize() {
        let json = "{\"duration\": \"-01:-02:-03\"}";
        let Err(err) = serde_json::from_str::<TimeDeltaWrapper>(json) else {
            panic!("Didn't get an error")
        };
        assert_eq!(
            err.to_string(),
            "Invalid time delta -01:-02:-03 at line 1 column 27"
        );
    }

    #[derive(Serialize, Deserialize)]
    struct OptionDurationWrapper {
        #[serde(with = "super::with_option_duration")]
        #[serde(default)]
        my_duration: Option<Duration>,
    }

    #[test]
    fn test_option_duration_serialize() {
        let duration = OptionDurationWrapper {
            my_duration: Some(duration::from_str("1:2:3").unwrap()),
        };
        let json = serde_json::to_string(&duration).unwrap();
        assert_eq!(json, "{\"my_duration\":\"01:02:03\"}");

        let duration = OptionDurationWrapper { my_duration: None };
        let json = serde_json::to_string(&duration).unwrap();
        assert_eq!(json, "{\"my_duration\":null}");
    }

    #[test]
    fn test_option_duration_deserialize() {
        let json = "{\"my_duration\":\"01:02:03\"}";
        let OptionDurationWrapper {
            my_duration: duration,
        }: OptionDurationWrapper = serde_json::from_str(json).unwrap();
        let duration = duration.unwrap();
        assert_eq!(duration.as_secs(), (60 + 2) * 60 + 3);

        let json = "{\"my_duration\":null}";
        let OptionDurationWrapper {
            my_duration: duration,
        }: OptionDurationWrapper = serde_json::from_str(json).unwrap();
        assert!(duration.is_none());

        let json = "{}";
        let OptionDurationWrapper {
            my_duration: duration,
        }: OptionDurationWrapper = serde_json::from_str(json).unwrap();
        assert!(duration.is_none());
    }

    #[derive(Serialize, Deserialize)]
    struct OptionTimeDeltaWrapper {
        #[serde(with = "super::with_option_time_delta")]
        duration: Option<TimeDelta>,
    }

    #[test]
    fn test_option_time_delta_serialize() {
        let duration = OptionTimeDeltaWrapper {
            duration: Some(time_delta::from_str("1:2:3").unwrap()),
        };
        let json = serde_json::to_string(&duration).unwrap();
        assert_eq!(json, "{\"duration\":\"01:02:03\"}");
    }

    #[test]
    fn test_option_time_delta_serialize_null() {
        let duration = OptionTimeDeltaWrapper { duration: None };
        let json = serde_json::to_string(&duration).unwrap();
        assert_eq!(json, "{\"duration\":null}");
    }

    #[test]
    fn test_option_time_delta_deserialize() {
        let json = "{\"duration\": \"01:02:03\"}";
        let OptionTimeDeltaWrapper { duration } = serde_json::from_str(json).unwrap();
        let duration = duration.unwrap();
        assert_eq!(duration.num_seconds(), (60 + 2) * 60 + 3);
        assert_eq!(duration.num_minutes(), 60 + 2);
        assert_eq!(duration.num_hours(), 1);
    }

    #[test]
    fn test_option_time_delta_deserialize_null() {
        let json = "{\"duration\": null}";
        let OptionTimeDeltaWrapper { duration } = serde_json::from_str(json).unwrap();
        assert!(duration.is_none());
    }

    #[test]
    fn test_date_num_days_from_ce() {
        // Ensure that this function returns values consistent with Elxir's to_iso_days/1 function.
        let date = Date::from_str("0000-01-01").unwrap();
        assert_eq!(num_days_from_ce(&date), 0);

        let date = Date::from_str("0000-01-02").unwrap();
        assert_eq!(num_days_from_ce(&date), 1);

        let date = Date::from_str("2020-01-01").unwrap();
        assert_eq!(num_days_from_ce(&date), 737_790);

        let date = Date::from_str("2021-01-01").unwrap();
        assert_eq!(num_days_from_ce(&date), 738_156);

        let date = Date::from_str("2022-01-01").unwrap();
        assert_eq!(num_days_from_ce(&date), 738_521);

        let date = Date::from_str("2023-01-01").unwrap();
        assert_eq!(num_days_from_ce(&date), 738_886);

        let date = Date::from_str("2023-03-06").unwrap();
        assert_eq!(num_days_from_ce(&date), 738_950);

        let date = Date::from_str("2023-04-03").unwrap();
        assert_eq!(num_days_from_ce(&date), 738_978);

        let date = Date::from_str("2023-05-01").unwrap();
        assert_eq!(num_days_from_ce(&date), 739_006);

        let date = Date::from_str("2023-05-15").unwrap();
        assert_eq!(num_days_from_ce(&date), 739_020);
    }
}
