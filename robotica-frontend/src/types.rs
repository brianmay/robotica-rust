//! Serializable types.
use std::{
    cmp::Ordering,
    fmt::{Display, Formatter},
    ops::{Add, Mul, Sub},
    str::FromStr,
};

use chrono::{Datelike, Local, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// A Serializable Date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Date(chrono::NaiveDate);

impl Date {
    /// Convert a year,month,day into a Date.
    #[must_use]
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Self {
        Self(chrono::NaiveDate::from_ymd(year, month, day))
    }

    /// Get the weekday of the date.
    #[must_use]
    pub fn weekday(self) -> Weekday {
        Weekday(self.0.weekday())
    }

    /// Get the number of days from CE of the date.
    ///
    // Kludge: This function returns values consistent with Elxir's to_iso_days/1 function.

    #[must_use]
    pub fn num_days_from_ce(self) -> i32 {
        self.0.num_days_from_ce() + 365
    }
}

impl Add<Duration> for Date {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub<Duration> for Date {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl FromStr for Date {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let date = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")?;
        Ok(Self(date))
    }
}

impl Display for Date {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.format("%Y-%m-%d"))
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        let d = Date::from_str(&s).map_err(serde::de::Error::custom)?;
        Ok(d)
    }
}

/// A Serializable Weekday.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Weekday(chrono::Weekday);

impl From<chrono::Weekday> for Weekday {
    fn from(wd: chrono::Weekday) -> Self {
        Self(wd)
    }
}

impl ToString for Weekday {
    fn to_string(&self) -> String {
        match self.0 {
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
}

impl<'de> Deserialize<'de> for Weekday {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;

        match s.to_lowercase().as_str() {
            "monday" => Ok(Weekday(chrono::Weekday::Mon)),
            "tuesday" => Ok(Weekday(chrono::Weekday::Tue)),
            "wednesday" => Ok(Weekday(chrono::Weekday::Wed)),
            "thursday" => Ok(Weekday(chrono::Weekday::Thu)),
            "friday" => Ok(Weekday(chrono::Weekday::Fri)),
            "saturday" => Ok(Weekday(chrono::Weekday::Sat)),
            "sunday" => Ok(Weekday(chrono::Weekday::Sun)),
            _ => Err(serde::de::Error::custom(format!("Invalid weekday {s}"))),
        }
    }
}

/// A Serializable Time.
#[derive(Debug, Copy, Clone)]
pub struct Time(chrono::NaiveTime);

impl FromStr for Time {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let date = chrono::NaiveTime::parse_from_str(s, "%H:%M:%S")?;
        Ok(Self(date))
    }
}

impl<'de> Deserialize<'de> for Time {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        let d = Time::from_str(&s).map_err(serde::de::Error::custom)?;
        Ok(d)
    }
}

// /// An error that can occur when translating a `DateTime`.
// #[derive(Error, Debug)]
// pub enum DateTimeError<T: TimeZone> {
//     /// There are no options converting this date time to a local date time.
//     #[error("Can't convert DateTime {0} to local timezone")]
//     CantConvertDateTime(NaiveDateTime),

//     /// There are multiple options converting this date time to a local date time.
//     #[error("DateTime {0} is ambiguous when converting to local timezone")]
//     AmbiguousDateTime(NaiveDateTime, DateTime<T>, DateTime<T>),
// }

/// A Serializable `DateTime`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DateTime<Tz: TimeZone>(chrono::DateTime<Tz>);

impl<T: TimeZone> DateTime<T> {
    /// Get the date of the date time.
    #[must_use]
    pub fn date(self) -> Date {
        Date(self.0.date().naive_local())
    }

    /// Get the time of the date time.
    #[must_use]
    pub fn time(self) -> Time {
        Time(self.0.time())
    }
}

// /// Get the current time in UTC timezone.
// #[must_use]
// pub fn utc_now() -> DateTime<Utc> {
//     DateTime(Utc::now())
// }

impl std::fmt::Display for DateTime<Utc> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let local = self.0.with_timezone(&Local);
        write!(f, "{}", local.format("%Y-%m-%d %H:%M:%S %z"))
    }
}

impl<T: TimeZone> std::fmt::Debug for DateTime<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let local = self.0.with_timezone(&Local);
        write!(f, "{}", local.format("DateTime(%Y-%m-%d %H:%M:%S %z)"))
    }
}

impl DateTime<Utc> {
    /// Convert the date time to a local date time.
    pub fn with_timezone<T: TimeZone>(&self, tz: &T) -> DateTime<T> {
        DateTime(self.0.with_timezone(tz))
    }
}

impl<T: TimeZone> From<chrono::DateTime<T>> for DateTime<T> {
    fn from(dt: chrono::DateTime<T>) -> Self {
        Self(dt)
    }
}

impl FromStr for DateTime<Utc> {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let datetime = chrono::DateTime::parse_from_rfc3339(s)?;
        let datetime = datetime.with_timezone(&Utc);
        Ok(datetime.into())
    }
}

impl Ord for DateTime<Utc> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd<DateTime<Utc>> for DateTime<Utc> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Add<Duration> for DateTime<Utc> {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub<Duration> for DateTime<Utc> {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Sub<DateTime<Utc>> for DateTime<Utc> {
    type Output = Duration;

    fn sub(self, rhs: DateTime<Utc>) -> Self::Output {
        Duration(self.0 - rhs.0)
    }
}

impl<'de> Deserialize<'de> for DateTime<Utc> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        let d = DateTime::from_str(&s).map_err(serde::de::Error::custom)?;
        Ok(d)
    }
}

impl Serialize for DateTime<Utc> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = self.0.to_rfc3339();
        serializer.serialize_str(&s)
    }
}

/// The value was out of range.
#[derive(Error, Debug)]
pub struct OutOfRangeError {}

impl Display for OutOfRangeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Out of range")
    }
}

/// A Serializable Duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Duration(chrono::Duration);

impl Duration {
    /// Create a new Duration.
    #[must_use]
    pub fn new(hours: i64, minutes: i64, seconds: i64) -> Self {
        Self(
            chrono::Duration::hours(hours)
                + chrono::Duration::minutes(minutes)
                + chrono::Duration::seconds(seconds),
        )
    }

    /// Get the number of hours in the duration.
    #[must_use]
    pub fn num_hours(self) -> i64 {
        self.0.num_hours()
    }

    /// Get the number of minutes in the duration.
    #[must_use]
    pub fn num_minutes(self) -> i64 {
        self.0.num_minutes()
    }

    /// Get the number of seconds in the duration.
    #[must_use]
    pub fn num_seconds(self) -> i64 {
        self.0.num_seconds()
    }

    /// Create a duration from a number of days.
    #[must_use]
    pub fn days(days: i64) -> Self {
        Self(chrono::Duration::days(days))
    }

    /// Create a duration from a number of seconds.
    #[must_use]
    pub fn seconds(seconds: i64) -> Self {
        Self(chrono::Duration::seconds(seconds))
    }

    /// Create a duration from a number of minutes.
    #[must_use]
    pub fn minutes(minutes: i64) -> Self {
        Self(chrono::Duration::minutes(minutes))
    }

    // /// Convert the duration to a std duration.
    // ///
    // /// # Errors
    // ///
    // /// If the duration is negative.
    // pub fn to_std(&self) -> Result<std::time::Duration, OutOfRangeError> {
    //     self.0.to_std().map_err(|_| OutOfRangeError {})
    // }
}

impl From<chrono::Duration> for Duration {
    fn from(d: chrono::Duration) -> Self {
        Self(d)
    }
}

impl Serialize for Duration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = self.0.to_std().map_err(serde::ser::Error::custom)?;
        serializer.serialize_u64(s.as_secs())
    }
}

/// A Serializable Duration.
#[derive(Error, Debug)]
pub enum DurationParseError {
    /// The duration is invalid.
    #[error("Invalid duration {0}")]
    InvalidDuration(String),
}

impl FromStr for Duration {
    type Err = DurationParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let splits = s.split(':').collect::<Vec<&str>>();
        if splits.len() == 2 {
            let hours = splits[0]
                .parse::<i64>()
                .map_err(|_| DurationParseError::InvalidDuration(s.to_string()))?;
            let minutes = splits[1]
                .parse::<i64>()
                .map_err(|_| DurationParseError::InvalidDuration(s.to_string()))?;
            Ok(Duration::new(hours, minutes, 0))
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
            Ok(Duration::new(hours, minutes, seconds))
        } else {
            Err(DurationParseError::InvalidDuration(s.to_string()))
        }
    }
}

impl<'de> Deserialize<'de> for Duration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        let d = Duration::from_str(&s).map_err(serde::de::Error::custom)?;
        Ok(d)
    }
}

impl Mul<i32> for Duration {
    type Output = Duration;

    fn mul(self, rhs: i32) -> Self::Output {
        Duration(self.0 * rhs)
    }
}

// /// Convert a Date and a Time to a Utc `DateTime`.
// ///
// /// # Errors
// ///
// /// If the date and time cannot be converted to a local date time.
// pub fn convert_date_time_to_utc<T: TimeZone>(
//     date: Date,
//     time: Time,
//     timezone: &T,
// ) -> Result<DateTime<chrono::Utc>, DateTimeError<T>> {
//     let date = date.0;
//     let time = time.0;
//     let datetime = chrono::NaiveDateTime::new(date, time);
//     let datetime = match timezone.from_local_datetime(&datetime) {
//         chrono::LocalResult::None => Err(DateTimeError::CantConvertDateTime(datetime)),
//         chrono::LocalResult::Single(datetime) => Ok(datetime),
//         chrono::LocalResult::Ambiguous(x, y) => Err(DateTimeError::AmbiguousDateTime(
//             datetime,
//             x.into(),
//             y.into(),
//         )),
//     }?;
//     Ok(datetime.with_timezone(&chrono::Utc).into())
// }

/// The status of the Mark.
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub enum MarkStatus {
    /// The tasks are to be cancelled.
    #[serde(rename = "cancelled")]
    Cancelled,

    /// The tasks are already done.
    #[serde(rename = "done")]
    Done,
}

/// A mark on a step.
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct Mark {
    /// The id of the step.
    pub id: String,

    /// The status of the Mark.
    pub status: MarkStatus,

    /// The start time of the Mark.
    pub start_time: DateTime<Utc>,

    /// The end time of the Mark.
    pub stop_time: DateTime<Utc>,
}

/// An error that can occur when parsing a mark.
#[derive(Error, Debug)]
pub enum MarkError {
    /// The Mark is invalid.
    #[error("Invalid mark {0}")]
    ParseError(#[from] serde_json::Error),

    /// UTF-8 error in Mark.
    #[error("Invalid UTF8")]
    Utf8Error(#[from] std::str::Utf8Error),
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    // use chrono::FixedOffset;

    use super::*;

    // #[test]
    // fn test_convert_date_time_to_utc() {
    //     let date = Date::from_str("2020-01-01").unwrap();
    //     let time = Time::from_str("10:00:00").unwrap();
    //     let timezone = FixedOffset::east(60 * 60 * 10);
    //     let datetime = convert_date_time_to_utc(date, time, &timezone).unwrap();
    //     assert_eq!(
    //         datetime,
    //         chrono::Utc.ymd(2020, 1, 1).and_hms(00, 0, 0).into()
    //     );
    // }

    #[test]
    fn test_duration() {
        let duration = Duration::from_str("1:2").unwrap();
        assert_eq!(duration.num_hours(), 1);
        assert_eq!(duration.num_minutes(), 60 + 2);
        assert_eq!(duration.num_seconds(), (60 + 2) * 60);

        let duration = Duration::from_str("1:2:3").unwrap();
        assert_eq!(duration.num_hours(), 1);
        assert_eq!(duration.num_minutes(), 60 + 2);
        assert_eq!(duration.num_seconds(), (60 + 2) * 60 + 3);

        let duration = Duration::from_str("1:2:3:4");
        assert!(matches!(
            duration,
            Err(DurationParseError::InvalidDuration(_))
        ));

        let duration = Duration::from_str("1");
        assert!(matches!(
            duration,
            Err(DurationParseError::InvalidDuration(_))
        ));

        let duration = Duration::from_str("1:a:3:4");
        assert!(matches!(
            duration,
            Err(DurationParseError::InvalidDuration(_))
        ));
    }

    #[test]
    fn test_date_num_days_from_ce() {
        // Ensure that this function returns values consistent with Elxir's to_iso_days/1 function.
        let date = Date::from_str("0000-01-01").unwrap();
        assert_eq!(date.num_days_from_ce(), 0);

        let date = Date::from_str("0000-01-02").unwrap();
        assert_eq!(date.num_days_from_ce(), 1);

        let date = Date::from_str("2020-01-01").unwrap();
        assert_eq!(date.num_days_from_ce(), 737_790);

        let date = Date::from_str("2021-01-01").unwrap();
        assert_eq!(date.num_days_from_ce(), 738_156);

        let date = Date::from_str("2022-01-01").unwrap();
        assert_eq!(date.num_days_from_ce(), 738_521);
    }
}
