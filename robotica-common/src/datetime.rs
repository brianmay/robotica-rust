//! Serializable datetime types.

use chrono::{Datelike, Local, NaiveDateTime, TimeZone, Utc};
use thiserror::Error;

/// A Date.
pub type Date = chrono::NaiveDate;

/// A Time.
pub type Time = chrono::NaiveTime;

/// A Date and a Time for a given Timezone.
pub type DateTime<Tz> = chrono::DateTime<Tz>;

/// A day of the week.
pub type Weekday = chrono::Weekday;

/// A duration between two times.
pub type Duration = chrono::Duration;

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

/// Create a new Duration.
#[must_use]
pub fn duration_from_hms(hours: i64, minutes: i64, seconds: i64) -> Duration {
    chrono::Duration::hours(hours)
        + chrono::Duration::minutes(minutes)
        + chrono::Duration::seconds(seconds)
}

const fn div_rem(a: u64, b: u64) -> (u64, u64) {
    (a / b, a % b)
}

/// A Serializable Duration.
#[derive(Error, Debug)]
pub enum DurationParseError {
    /// The duration is invalid.
    #[error("Invalid duration {0}")]
    InvalidDuration(String),
}

fn duration_from_str(s: &str) -> Result<Duration, DurationParseError> {
    let splits = s.split(':').collect::<Vec<&str>>();
    if splits.len() == 2 {
        let hours = splits[0]
            .parse::<i64>()
            .map_err(|_| DurationParseError::InvalidDuration(s.to_string()))?;
        let minutes = splits[1]
            .parse::<i64>()
            .map_err(|_| DurationParseError::InvalidDuration(s.to_string()))?;
        Ok(duration_from_hms(hours, minutes, 0))
    } else if splits.len() == 3 {
        let hours = splits[0]
            .parse::<i64>()
            .map_err(|_| DurationParseError::InvalidDuration(s.to_string()))?;
        let minutes = splits[1]
            .parse::<i64>()
            .map_err(|_| DurationParseError::InvalidDuration(s.to_string()))?;
        let seconds = splits[2]
            .parse::<i64>()
            .map_err(|_| DurationParseError::InvalidDuration(s.to_string()))?;
        Ok(duration_from_hms(hours, minutes, seconds))
    } else {
        Err(DurationParseError::InvalidDuration(s.to_string()))
    }
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
        let d = super::duration_from_str(&s).map_err(serde::de::Error::custom)?;
        Ok(d)
    }

    /// Serialize a duration.
    ///
    /// # Errors
    ///
    /// If the duration is negative.
    pub fn serialize<S>(duration: &super::Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = duration.to_std().map_err(serde::ser::Error::custom)?;

        let secs = s.as_secs();

        let (minutes, secs) = super::div_rem(secs, 60);
        let (hours, minutes) = super::div_rem(minutes, 60);

        let result = format!("{hours:02}:{minutes:02}:{secs:02}");
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
        .or_else(|| convert(time - Duration::hours(1)))
        .or_else(|| convert(time + Duration::hours(1)))
        .unwrap_or_else(|| {
            let datetime = chrono::NaiveDateTime::new(date, time);
            chrono::Utc
                .from_utc_datetime(&datetime)
                .with_timezone(&chrono::Utc)
        })
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
    fn test_duration() {
        let duration = duration_from_str("1:2").unwrap();
        assert_eq!(duration.num_hours(), 1);
        assert_eq!(duration.num_minutes(), 60 + 2);
        assert_eq!(duration.num_seconds(), (60 + 2) * 60);

        let duration = duration_from_str("1:2:3").unwrap();
        assert_eq!(duration.num_hours(), 1);
        assert_eq!(duration.num_minutes(), 60 + 2);
        assert_eq!(duration.num_seconds(), (60 + 2) * 60 + 3);

        let duration = duration_from_str("1:2:3:4");
        assert!(matches!(
            duration,
            Err(DurationParseError::InvalidDuration(_))
        ));

        let duration = duration_from_str("1");
        assert!(matches!(
            duration,
            Err(DurationParseError::InvalidDuration(_))
        ));

        let duration = duration_from_str("1:a:3:4");
        assert!(matches!(
            duration,
            Err(DurationParseError::InvalidDuration(_))
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
            duration: duration_from_str("1:2:3").unwrap(),
        };
        let json = serde_json::to_string(&duration).unwrap();
        assert_eq!(json, "{\"duration\":\"01:02:03\"}");
    }

    #[test]
    fn test_duration_deserialize() {
        let json = "{\"duration\":\"01:02:03\"}";
        let DurationWrapper { duration }: DurationWrapper = serde_json::from_str(json).unwrap();
        assert_eq!(duration.num_hours(), 1);
        assert_eq!(duration.num_minutes(), 60 + 2);
        assert_eq!(duration.num_seconds(), (60 + 2) * 60 + 3);
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
            my_duration: Some(duration_from_str("1:2:3").unwrap()),
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
        assert_eq!(duration.num_hours(), 1);
        assert_eq!(duration.num_minutes(), 60 + 2);
        assert_eq!(duration.num_seconds(), (60 + 2) * 60 + 3);

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
