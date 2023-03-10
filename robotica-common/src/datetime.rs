//! Serializable datetime types.

use chrono::{Datelike, NaiveDateTime, TimeZone, Utc};
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

// /// A Serializable Date.
// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
// pub struct Date(chrono::NaiveDate);

// impl Date {
//     /// Convert a year,month,day into a Date.
//     #[must_use]
//     pub fn from_ymd_opt(year: i32, month: u32, day: u32) -> Option<Self> {
//         chrono::NaiveDate::from_ymd_opt(year, month, day).map(Self)
//     }

//     /// Get the weekday of the date.
//     #[must_use]
//     pub fn weekday(self) -> Weekday {
//         Weekday(self.0.weekday())
//     }

/// Get the number of days from CE of the date.
///
// Kludge: This function returns values consistent with Elxir's to_iso_days/1 function.
#[must_use]
pub fn num_days_from_ce(date: &Date) -> i32 {
    date.num_days_from_ce() + 365
}

// }

// impl Add<Duration> for Date {
//     type Output = Self;

//     fn add(self, rhs: Duration) -> Self::Output {
//         self + rhs.0
//     }
// }

// impl Sub<Duration> for Date {
//     type Output = Self;

//     fn sub(self, rhs: Duration) -> Self::Output {
//         self - rhs.0
//     }
// }

// impl FromStr for Date {
//     type Err = chrono::ParseError;

//     fn from_str(s: &str) -> Result<Self, Self::Err> {
//         let date = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")?;
//         Ok(Self(date))
//     }
// }

// impl Display for Date {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{}", self.0.format("%Y-%m-%d"))
//     }
// }

// impl<'de> Deserialize<'de> for Date {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         let s: String = Deserialize::deserialize(deserializer)?;
//         let d = Date::from_str(&s).map_err(serde::de::Error::custom)?;
//         Ok(d)
//     }
// }

// /// A Serializable Weekday.
// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
// pub struct Weekday(chrono::Weekday);

// impl From<chrono::Weekday> for Weekday {
//     fn from(wd: chrono::Weekday) -> Self {
//         Self(wd)
//     }
// }

// impl ToString for Weekday {
//     fn to_string(&self) -> String {
//         match self.0 {
//             chrono::Weekday::Mon => "Monday",
//             chrono::Weekday::Tue => "Tuesday",
//             chrono::Weekday::Wed => "Wednesday",
//             chrono::Weekday::Thu => "Thursday",
//             chrono::Weekday::Fri => "Friday",
//             chrono::Weekday::Sat => "Saturday",
//             chrono::Weekday::Sun => "Sunday",
//         }
//         .to_string()
//     }
// }

// impl<'de> Deserialize<'de> for Weekday {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         let s: String = Deserialize::deserialize(deserializer)?;

//         match s.to_lowercase().as_str() {
//             "monday" => Ok(Weekday(chrono::Weekday::Mon)),
//             "tuesday" => Ok(Weekday(chrono::Weekday::Tue)),
//             "wednesday" => Ok(Weekday(chrono::Weekday::Wed)),
//             "thursday" => Ok(Weekday(chrono::Weekday::Thu)),
//             "friday" => Ok(Weekday(chrono::Weekday::Fri)),
//             "saturday" => Ok(Weekday(chrono::Weekday::Sat)),
//             "sunday" => Ok(Weekday(chrono::Weekday::Sun)),
//             _ => Err(serde::de::Error::custom(format!("Invalid weekday {s}"))),
//         }
//     }
// }

// /// A Serializable Time.
// #[derive(Debug, Copy, Clone, Default)]
// pub struct Time(chrono::NaiveTime);

// impl Time {
//     #[must_use]
//     /// Create a new Time.
//     pub fn new(hour: u32, minute: u32, second: u32) -> Option<Self> {
//         chrono::NaiveTime::from_hms_opt(hour, minute, second).map(Self)
//     }
// }

// impl FromStr for Time {
//     type Err = chrono::ParseError;

//     fn from_str(s: &str) -> Result<Self, Self::Err> {
//         let date = chrono::NaiveTime::parse_from_str(s, "%H:%M:%S")?;
//         Ok(Self(date))
//     }
// }

// impl Display for Time {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         self.0.fmt(f)
//     }
// }

// impl Add<Duration> for Time {
//     type Output = Self;

//     fn add(self, rhs: Duration) -> Self::Output {
//         self + rhs.0
//     }
// }

// impl Sub<Duration> for Time {
//     type Output = Self;

//     fn sub(self, rhs: Duration) -> Self::Output {
//         self - rhs.0
//     }
// }

// impl<'de> Deserialize<'de> for Time {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         let s: String = Deserialize::deserialize(deserializer)?;
//         let d = Time::from_str(&s).map_err(serde::de::Error::custom)?;
//         Ok(d)
//     }
// }

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

// /// A Serializable `DateTime`.
// #[derive(Clone, PartialEq, Eq, Hash)]
// pub struct DateTime<Tz: TimeZone>(chrono::DateTime<Tz>);

// impl<T: TimeZone> DateTime<T> {
//     /// Get the date of the date time.
//     #[must_use]
//     pub fn date(self) -> Date {
//         Date(self.0.date_naive())
//     }

//     /// Get the time of the date time.
//     #[must_use]
//     pub fn time(self) -> Time {
//         Time(self.0.time())
//     }
// }

/// Get the current time in UTC timezone.
#[must_use]
pub fn utc_now() -> DateTime<Utc> {
    Utc::now()
}

// impl std::fmt::Display for DateTime<Utc> {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         let local = self.0.with_timezone(&Local);
//         write!(f, "{}", local.format("%Y-%m-%d %H:%M:%S %z"))
//     }
// }

// impl<T: TimeZone> std::fmt::Debug for DateTime<T> {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         let local = self.0.with_timezone(&Local);
//         write!(f, "{}", local.format("DateTime(%Y-%m-%d %H:%M:%S %z)"))
//     }
// }

// impl DateTime<Utc> {
//     /// Convert the date time to a local date time.
//     pub fn with_timezone<T: TimeZone>(&self, tz: &T) -> DateTime<T> {
//         DateTime(self.0.with_timezone(tz))
//     }
// }

// impl<T: TimeZone> From<chrono::DateTime<T>> for DateTime<T> {
//     fn from(dt: chrono::DateTime<T>) -> Self {
//         Self(dt)
//     }
// }

// impl<T: TimeZone> From<DateTime<T>> for chrono::DateTime<T> {
//     fn from(dt: DateTime<T>) -> Self {
//         dt.0
//     }
// }

// impl FromStr for DateTime<Utc> {
//     type Err = chrono::ParseError;

//     fn from_str(s: &str) -> Result<Self, Self::Err> {
//         let datetime = chrono::DateTime::parse_from_rfc3339(s)?;
//         let datetime = datetime.with_timezone(&Utc);
//         Ok(datetime.into())
//     }
// }

// impl Ord for DateTime<Utc> {
//     fn cmp(&self, other: &Self) -> Ordering {
//         self.0.cmp(&other.0)
//     }
// }

// impl PartialOrd<DateTime<Utc>> for DateTime<Utc> {
//     fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
//         self.0.partial_cmp(&other.0)
//     }
// }

// impl Add<Duration> for DateTime<Utc> {
//     type Output = Self;

//     fn add(self, rhs: Duration) -> Self::Output {
//         self + rhs.0
//     }
// }

// impl Sub<Duration> for DateTime<Utc> {
//     type Output = Self;

//     fn sub(self, rhs: Duration) -> Self::Output {
//         self - rhs.0
//     }
// }

// // impl Sub<DateTime<Utc>> for DateTime<Utc> {
// //     type Output = Duration;

// //     fn sub(self, rhs: DateTime<Utc>) -> Self::Output {
// //         Duration(self.0 - rhs.0)
// //     }
// // }

// // impl<'de> Deserialize<'de> for DateTime<Utc> {
// //     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
// //     where
// //         D: Deserializer<'de>,
// //     {
// //         let s: String = Deserialize::deserialize(deserializer)?;
// //         let d = DateTime::from_str(&s).map_err(serde::de::Error::custom)?;
// //         Ok(d)
// //     }
// // }

// // impl Serialize for DateTime<Utc> {
// //     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
// //     where
// //         S: Serializer,
// //     {
// //         let s = self.0.to_rfc3339();
// //         serializer.serialize_str(&s)
// //     }
// // }

// /// The value was out of range.
// #[derive(Error, Debug)]
// pub struct OutOfRangeError {}

// impl Display for OutOfRangeError {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         write!(f, "Out of range")
//     }
// }

// /// A Serializable Duration.
// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
// pub struct Duration(chrono::Duration);

// impl Duration {

/// Create a new Duration.
#[must_use]
pub fn duration_from_hms(hours: i64, minutes: i64, seconds: i64) -> Duration {
    chrono::Duration::hours(hours)
        + chrono::Duration::minutes(minutes)
        + chrono::Duration::seconds(seconds)
}

//     /// Get the number of hours in the duration.
//     #[must_use]
//     pub fn num_hours(self) -> i64 {
//         self.0.num_hours()
//     }

//     /// Get the number of minutes in the duration.
//     #[must_use]
//     pub fn num_minutes(self) -> i64 {
//         self.0.num_minutes()
//     }

//     /// Get the number of seconds in the duration.
//     #[must_use]
//     pub fn num_seconds(self) -> i64 {
//         self.0.num_seconds()
//     }

//     /// Create a duration from a number of days.
//     #[must_use]
//     pub fn days(days: i64) -> Self {
//         Self(chrono::Duration::days(days))
//     }

//     /// Create a duration from a number of seconds.
//     #[must_use]
//     pub fn seconds(seconds: i64) -> Self {
//         Self(chrono::Duration::seconds(seconds))
//     }

//     /// Create a duration from a number of minutes.
//     #[must_use]
//     pub fn minutes(minutes: i64) -> Self {
//         Self(chrono::Duration::minutes(minutes))
//     }

//     /// Create a duration from a number of hours.
//     #[must_use]
//     pub fn hours(hours: i64) -> Self {
//         Self(chrono::Duration::hours(hours))
//     }

//     /// Subtract a duration from this duration.
//     #[must_use]
//     pub fn checked_sub(self, rhs: &Self) -> Option<Self> {
//         self.0.checked_sub(&rhs.0).map(Self)
//     }

//     /// Convert the duration to a std duration.
//     ///
//     /// # Errors
//     ///
//     /// If the duration is negative.
//     pub fn to_std(&self) -> Result<std::time::Duration, OutOfRangeError> {
//         self.0.to_std().map_err(|_| OutOfRangeError {})
//     }
// }

// impl From<chrono::Duration> for Duration {
//     fn from(d: chrono::Duration) -> Self {
//         Self(d)
//     }
// }

// impl Add<Duration> for Duration {
//     type Output = Self;

//     fn add(self, rhs: Self) -> Self::Output {
//         Self(self.0 + rhs.0)
//     }
// }

// impl std::fmt::Display for Duration {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{}", self.0)
//     }
// }

const fn div_rem(a: u64, b: u64) -> (u64, u64) {
    (a / b, a % b)
}

// impl Serialize for Duration {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         let s = self.0.to_std().map_err(serde::ser::Error::custom)?;

//         let secs = s.as_secs();

//         let (minutes, secs) = div_rem(secs, 60);
//         let (hours, minutes) = div_rem(minutes, 60);

//         let result = format!("{hours:02}:{minutes:02}:{secs:02}");
//         serializer.serialize_str(&result)
//     }
// }

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

// impl<'de> Deserialize<'de> for Duration {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         let s: String = Deserialize::deserialize(deserializer)?;
//         let d = Duration::from_str(&s).map_err(serde::de::Error::custom)?;
//         Ok(d)
//     }
// }

// impl Mul<i32> for Duration {
//     type Output = Duration;

//     fn mul(self, rhs: i32) -> Self::Output {
//         Duration(self.0 * rhs)
//     }
// }

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
    }
}
