//! Provide ability to load from iCal calendars with recurring event support.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use icalendar::{
    Calendar as IcalCalendar, CalendarDateTime, Component, DatePerhapsTime, EventLike,
};
use thiserror::Error;

/// A calendar entry representing an event from an iCal calendar.
#[derive(Debug)]
pub struct CalendarEntry {
    /// The event summary/title
    pub summary: String,
    /// The event description
    pub description: Option<String>,
    /// The event location
    pub location: Option<String>,
    /// The event UID
    pub uid: String,
    /// The event status
    pub status: Option<String>,
    /// Whether this is an all-day event (no specific time)
    pub is_all_day: bool,
    /// The start time of the event in UTC
    pub start: DateTime<Utc>,
    /// The end time of the event in UTC
    pub end: DateTime<Utc>,
}

#[allow(clippy::expect_used)]
fn naive_date_to_datetime<T: TimeZone>(date: NaiveDate, tz: &T) -> DateTime<T> {
    let naive_dt = date
        .and_hms_opt(0, 0, 0)
        .expect("hour, minute, and second are valid values");
    tz.from_local_datetime(&naive_dt).unwrap()
}

fn calendar_datetime_to_utc(dt: &CalendarDateTime) -> Option<DateTime<Utc>> {
    match dt {
        CalendarDateTime::Utc(dt) => Some(*dt),
        CalendarDateTime::WithTimezone { date_time, tzid } => {
            tzid.parse::<chrono_tz::Tz>().map_or_else(
                |_| Some(Utc.from_utc_datetime(date_time)),
                |tz| {
                    tz.from_local_datetime(date_time)
                        .single()
                        .map(|dt| dt.with_timezone(&Utc))
                },
            )
        }
        CalendarDateTime::Floating(naive_dt) => Some(Utc.from_utc_datetime(naive_dt)),
    }
}

/// Parse a calendar from an iCal string and extract events within a date range.
///
/// Note: For recurring events (RRULE), this function uses `after()` and `before()` bounds
/// to efficiently skip to the query window. However, there is a maximum limit of 10,000
/// occurrences that can be generated per event to prevent runaway recursion. In practice,
/// this should not be a problem since the `after()`/`before()` bounds filter to only
/// occurrences within the query range, and typical queries are for a finite date range.
///
/// # Errors
///
/// Returns an error if the calendar string cannot be parsed.
#[allow(clippy::too_many_lines)]
pub fn from_str<T: TimeZone>(
    ical_str: &str,
    start: NaiveDate,
    stop: NaiveDate,
    tz: &T,
) -> Result<Vec<CalendarEntry>, Error> {
    let calendar = ical_str.parse::<IcalCalendar>().map_err(|_| Error::Ical)?;

    let mut entries = Vec::new();
    let start_dt: DateTime<T> = naive_date_to_datetime(start, tz);
    let stop_dt: DateTime<T> = naive_date_to_datetime(stop, tz);
    let start_dt_utc = start_dt.with_timezone(&Utc);
    let stop_dt_utc = stop_dt.with_timezone(&Utc);
    let days_in_range = u16::try_from((stop - start).num_days()).unwrap_or(u16::MAX);
    let rrule_limit = days_in_range
        .saturating_mul(2)
        .saturating_add(10)
        .clamp(1000, 10000);
    let rrule_tz: icalendar::rrule::Tz = icalendar::rrule::Tz::UTC;

    for component in calendar.iter() {
        if let icalendar::CalendarComponent::Event(event) = component {
            if event.property_value("RECURRENCE-ID").is_some() {
                continue;
            }

            let event_start_opt = event.get_start();
            let event_end_opt = event.get_end();

            let (duration, entry_start_dt, is_all_day) = match (event_start_opt, event_end_opt) {
                (Some(DatePerhapsTime::DateTime(s)), Some(DatePerhapsTime::DateTime(e))) => {
                    let s_utc = calendar_datetime_to_utc(&s).unwrap_or(start_dt_utc);
                    let e_utc = calendar_datetime_to_utc(&e)
                        .unwrap_or_else(|| start_dt_utc + Duration::hours(1));
                    (e_utc - s_utc, Some(DatePerhapsTime::DateTime(s)), false)
                }
                (Some(DatePerhapsTime::Date(s)), Some(DatePerhapsTime::Date(e))) => {
                    let dur = Duration::days((e - s).num_days());
                    (dur, Some(DatePerhapsTime::Date(s)), true)
                }
                _ => (Duration::hours(1), None, false),
            };

            if let Ok(rrule_set) = event.get_recurrence() {
                let start_for_rrule = start_dt_utc.with_timezone(&rrule_tz);
                let stop_for_rrule = stop_dt_utc.with_timezone(&rrule_tz);
                let bounded = rrule_set.after(start_for_rrule).before(stop_for_rrule);
                let result = bounded.all(rrule_limit);

                for occurrence in result.dates {
                    let occurrence_utc = occurrence.with_timezone(&Utc);
                    if occurrence_utc < start_dt_utc || occurrence_utc >= stop_dt_utc {
                        continue;
                    }
                    let occurrence_end = occurrence_utc + duration;

                    let summary = event
                        .get_summary()
                        .map(ToString::to_string)
                        .unwrap_or_default();
                    let description = event.get_description().map(ToString::to_string);
                    let location = event.get_location().map(ToString::to_string);
                    let uid = event.get_uid().map(ToString::to_string).unwrap_or_default();
                    let status = event.get_status().map(|s| format!("{s:?}"));

                    entries.push(CalendarEntry {
                        summary,
                        description,
                        location,
                        uid,
                        status,
                        is_all_day,
                        start: occurrence_utc,
                        end: occurrence_end,
                    });
                }
            } else if let Some(s) = entry_start_dt {
                let s_utc = match s {
                    DatePerhapsTime::DateTime(dt) => {
                        calendar_datetime_to_utc(&dt).unwrap_or(start_dt_utc)
                    }
                    DatePerhapsTime::Date(date) => {
                        naive_date_to_datetime(date, tz).with_timezone(&Utc)
                    }
                };
                if s_utc >= start_dt_utc && s_utc < stop_dt_utc {
                    let e = s_utc + duration;
                    let summary = event
                        .get_summary()
                        .map(ToString::to_string)
                        .unwrap_or_default();
                    let description = event.get_description().map(ToString::to_string);
                    let location = event.get_location().map(ToString::to_string);
                    let uid = event.get_uid().map(ToString::to_string).unwrap_or_default();
                    let status = event.get_status().map(|s| format!("{s:?}"));

                    entries.push(CalendarEntry {
                        summary,
                        description,
                        location,
                        uid,
                        status,
                        is_all_day,
                        start: s_utc,
                        end: e,
                    });
                }
            }
        }
    }

    entries.sort_by_key(|e| e.start);

    Ok(entries)
}

/// Load a calendar from a URL and extract events within a date range.
///
/// # Errors
///
/// Returns an error if the HTTP request fails or the calendar cannot be parsed.
pub async fn load<T: TimeZone + Clone>(
    url: &str,
    start: NaiveDate,
    stop: NaiveDate,
    tz: T,
) -> Result<Vec<CalendarEntry>, Error> {
    let text = reqwest::get(url).await?.error_for_status()?.text().await?;
    from_str(&text, start, stop, &tz)
}

/// Error type for calendar operations.
#[derive(Error, Debug)]
pub enum Error {
    /// Reqwest HTTP error
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// iCal parsing error
    #[error("iCal parsing error")]
    Ical,

    /// `RRule` parsing or generation error
    #[error("RRule error: {0}")]
    RRule(#[from] icalendar::rrule::RRuleError),
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use chrono_tz::Europe::Berlin;

    const TEST_CALENDAR: &str =
        include_str!("../../fixtures/recurring_events_changed_duration.ics");

    #[test]
    fn test_calendar() {
        let c = from_str(
            TEST_CALENDAR,
            NaiveDate::from_ymd_opt(2019, 3, 5).unwrap(),
            NaiveDate::from_ymd_opt(2019, 4, 1).unwrap(),
            &Berlin,
        )
        .unwrap();
        assert!(c.len() == 7);
    }

    #[test]
    fn test_calendar_stop_same_date() {
        let c = from_str(
            TEST_CALENDAR,
            NaiveDate::from_ymd_opt(2019, 3, 18).unwrap(),
            NaiveDate::from_ymd_opt(2019, 3, 18).unwrap(),
            &Berlin,
        )
        .unwrap();
        assert!(c.is_empty());
    }

    #[test]
    fn test_calendar_stop_next_day() {
        let c = from_str(
            TEST_CALENDAR,
            NaiveDate::from_ymd_opt(2019, 3, 18).unwrap(),
            NaiveDate::from_ymd_opt(2019, 3, 19).unwrap(),
            &Berlin,
        )
        .unwrap();
        assert!(c.len() == 1);
    }
}
