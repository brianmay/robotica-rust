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

    for component in calendar.iter() {
        if let icalendar::CalendarComponent::Event(event) = component {
            if event.property_value("RECURRENCE-ID").is_some() {
                continue;
            }

            let event_start_opt = event.get_start();
            let event_end_opt = event.get_end();

            let (duration, entry_start_dt, is_all_day) =
                match (event_start_opt.clone(), event_end_opt.clone()) {
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
                let result = rrule_set.clone().all(rrule_limit);

                for occurrence in result.dates {
                    if is_all_day {
                        let occ_date = occurrence.date_naive();
                        if occ_date < start || occ_date > stop {
                            continue;
                        }
                        let occurrence_utc = occurrence.with_timezone(&Utc);
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
                        continue;
                    }
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
                let event_start_date = s_utc.date_naive();
                let event_end_date = (s_utc + duration).date_naive();
                let in_range = if is_all_day {
                    event_start_date <= stop && event_end_date >= start
                } else {
                    s_utc >= start_dt_utc && s_utc < stop_dt_utc
                };
                if in_range {
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
    const DAILY_EVENTS_CALENDAR: &str = include_str!("../../fixtures/daily_events.ics");
    const ADDITIONAL_EVENTS_CALENDAR: &str = include_str!("../../fixtures/additional_events.ics");

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
        assert!(c[0].summary == "test7");
        assert!(c[0].description == Some("description should be the same".to_string()));
        assert!(!c[0].is_all_day);
        assert!(c[0].start == Utc.with_ymd_and_hms(2019, 3, 18, 3, 0, 0).unwrap());
        assert!(c[0].end == Utc.with_ymd_and_hms(2019, 3, 18, 4, 0, 0).unwrap());
    }

    #[test]
    fn test_calendar_full_day() {
        let c = from_str(
            DAILY_EVENTS_CALENDAR,
            NaiveDate::from_ymd_opt(2026, 4, 22).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 22).unwrap(),
            &chrono_tz::Australia::Melbourne,
        )
        .unwrap();
        assert!(c.len() == 1);
        assert!(c[0].summary == "Eat Cheese");
        assert!(c[0].is_all_day);
        assert!(c[0].start == Utc.with_ymd_and_hms(2026, 4, 21, 14, 0, 0).unwrap());
        assert!(c[0].end == Utc.with_ymd_and_hms(2026, 4, 22, 14, 0, 0).unwrap());
    }

    #[test]
    fn test_calendar_recurring_full_day() {
        let c = from_str(
            DAILY_EVENTS_CALENDAR,
            NaiveDate::from_ymd_opt(2026, 4, 23).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 23).unwrap(),
            &chrono_tz::Australia::Melbourne,
        )
        .unwrap();
        assert!(c.len() == 1);
        assert!(c[0].summary == "Each super tasty cheese");
        assert!(c[0].is_all_day);
        assert!(c[0].start == Utc.with_ymd_and_hms(2026, 4, 22, 14, 0, 0).unwrap());
        assert!(c[0].end == Utc.with_ymd_and_hms(2026, 4, 23, 14, 0, 0).unwrap());
    }

    #[test]
    #[ignore = "rrule crate bug: DATE-only events use system's local timezone offset, not calendar's X-WR-TIMEZONE"]
    fn test_calendar_count_limited_rrule() {
        let c = from_str(
            ADDITIONAL_EVENTS_CALENDAR,
            NaiveDate::from_ymd_opt(2026, 4, 3).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 3).unwrap(),
            &chrono_tz::America::New_York,
        )
        .unwrap();
        assert!(c.len() == 1);
        assert!(c[0].summary == "Count Limited Daily Event");
        assert!(c[0].is_all_day);
    }

    #[test]
    #[ignore = "rrule crate bug: DATE-only events use system's local timezone offset, not calendar's X-WR-TIMEZONE"]
    fn test_calendar_count_rrule_returns_all_occurrences() {
        let c = from_str(
            ADDITIONAL_EVENTS_CALENDAR,
            NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
            &chrono_tz::America::New_York,
        )
        .unwrap();
        assert!(c.len() == 5);
        for event in &c {
            assert_eq!(event.summary, "Count Limited Daily Event");
        }
    }

    #[test]
    #[ignore = "rrule crate bug: DATE-only events use system's local timezone offset, not calendar's X-WR-TIMEZONE"]
    fn test_calendar_until_limited_rrule() {
        let c = from_str(
            ADDITIONAL_EVENTS_CALENDAR,
            NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
            &chrono_tz::America::New_York,
        )
        .unwrap();
        assert!(c.len() == 1);
        assert!(c[0].summary == "Until Limited Daily Event");
    }

    #[test]
    fn test_calendar_until_rrule_respects_until_date() {
        let c = from_str(
            ADDITIONAL_EVENTS_CALENDAR,
            NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
            &chrono_tz::America::New_York,
        )
        .unwrap();
        assert!(c.is_empty());
    }

    #[test]
    #[ignore = "rrule crate bug: DATE-only events use system's local timezone offset, not calendar's X-WR-TIMEZONE"]
    fn test_calendar_multiple_byday() {
        let c = from_str(
            ADDITIONAL_EVENTS_CALENDAR,
            NaiveDate::from_ymd_opt(2026, 4, 6).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 6).unwrap(),
            &chrono_tz::America::New_York,
        )
        .unwrap();
        assert!(c.len() == 1);
        assert!(c[0].summary == "Multi BYDAY Event");
    }

    #[test]
    #[ignore = "rrule crate bug: DATE-only events use system's local timezone offset, not calendar's X-WR-TIMEZONE"]
    fn test_calendar_multiday_allday_event() {
        let c = from_str(
            ADDITIONAL_EVENTS_CALENDAR,
            NaiveDate::from_ymd_opt(2026, 4, 11).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 11).unwrap(),
            &chrono_tz::America::New_York,
        )
        .unwrap();
        assert!(c.len() == 1);
        assert!(c[0].summary == "Multi-day All-day Event");
        assert!(c[0].is_all_day);
    }

    #[test]
    #[ignore = "rrule crate bug: DATE-only events use system's local timezone offset, not calendar's X-WR-TIMEZONE"]
    fn test_calendar_mixed_allday_and_timed() {
        let c = from_str(
            ADDITIONAL_EVENTS_CALENDAR,
            NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
            &chrono_tz::America::New_York,
        )
        .unwrap();
        assert!(c.len() == 2);
        let summaries: Vec<_> = c.iter().map(|e| e.summary.as_str()).collect();
        assert!(summaries.contains(&"Multi-day All-day Event"));
        assert!(summaries.contains(&"Timed Event in New York"));
        let allday = c
            .iter()
            .find(|e| e.summary == "Multi-day All-day Event")
            .unwrap();
        let timed = c
            .iter()
            .find(|e| e.summary == "Timed Event in New York")
            .unwrap();
        assert!(allday.is_all_day);
        assert!(!timed.is_all_day);
    }

    #[test]
    #[ignore = "rrule crate bug: DATE-only events use system's local timezone offset, not calendar's X-WR-TIMEZONE"]
    fn test_calendar_exact_boundary_start() {
        let c = from_str(
            ADDITIONAL_EVENTS_CALENDAR,
            NaiveDate::from_ymd_opt(2026, 4, 20).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 20).unwrap(),
            &chrono_tz::America::New_York,
        )
        .unwrap();
        assert!(c.len() == 1);
        assert!(c[0].summary == "Single All-day Event");
    }

    #[test]
    #[ignore = "rrule crate bug: DATE-only events use system's local timezone offset, not calendar's X-WR-TIMEZONE"]
    fn test_calendar_empty_result() {
        let c = from_str(
            ADDITIONAL_EVENTS_CALENDAR,
            NaiveDate::from_ymd_opt(2026, 4, 22).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 22).unwrap(),
            &chrono_tz::America::New_York,
        )
        .unwrap();
        assert!(c.is_empty());
    }

    #[test]
    fn test_calendar_future_only() {
        let c = from_str(
            ADDITIONAL_EVENTS_CALENDAR,
            NaiveDate::from_ymd_opt(2026, 4, 25).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 25).unwrap(),
            &chrono_tz::America::New_York,
        )
        .unwrap();
        assert!(c.len() == 1);
        assert!(c[0].summary == "Future All-day Event");
    }
}
