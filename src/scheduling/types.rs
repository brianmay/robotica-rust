//! Serializable types.
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

use chrono::{DateTime, Datelike, NaiveDateTime, TimeZone};
use serde::{Deserialize, Deserializer};
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
    #[must_use]
    pub fn num_days_from_ce(self) -> i32 {
        self.0.num_days_from_ce()
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

/// Convert a Date and a Time to a Utc `DateTime`.
///
/// # Errors
///
/// If the date and time cannot be converted to a local date time.
pub fn convert_date_time_to_utc<T: TimeZone>(
    date: Date,
    time: Time,
    timezone: &T,
) -> Result<chrono::DateTime<chrono::Utc>, DateTimeError<T>> {
    let date = date.0;
    let time = time.0;
    let datetime = chrono::NaiveDateTime::new(date, time);
    let datetime = match timezone.from_local_datetime(&datetime) {
        chrono::LocalResult::None => Err(DateTimeError::CantConvertDateTime(datetime)),
        chrono::LocalResult::Single(datetime) => Ok(datetime),
        chrono::LocalResult::Ambiguous(x, y) => {
            Err(DateTimeError::AmbiguousDateTime(datetime, x, y))
        }
    }?;
    Ok(datetime.with_timezone(&chrono::Utc))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use chrono::FixedOffset;

    use super::*;

    #[test]
    fn test_convert_date_time_to_utc() {
        let date = Date::from_str("2020-01-01").unwrap();
        let time = Time::from_str("10:00:00").unwrap();
        let timezone = FixedOffset::east(60 * 60 * 10);
        let datetime = convert_date_time_to_utc(date, time, &timezone).unwrap();
        assert_eq!(datetime, chrono::Utc.ymd(2020, 1, 1).and_hms(00, 0, 0));
    }
}
