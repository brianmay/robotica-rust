//! Provide ability to load from iCal calendars with recurring event support.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use icalendar::{Calendar as IcalCalendar, CalendarDateTime, Component, DatePerhapsTime, EventLike};
use thiserror::Error;

/// Represents a datetime value, either UTC or a date-only.
#[derive(Debug, Clone)]
pub enum Dt {
    /// A datetime in UTC
    DateTime(DateTime<Utc>),
    /// A date-only value
    Date(NaiveDate),
}

/// Represents the start and end of an event, either as dates or datetimes.
#[derive(Debug)]
pub enum StartEnd {
    /// Start and end are dates
    Date(NaiveDate, NaiveDate),
    /// Start and end are datetimes in UTC
    DateTime(DateTime<Utc>, DateTime<Utc>),
}

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
    /// The event transparency
    pub transp: String,
    /// The event sequence number
    pub sequence: u8,
    /// The start and end of the event
    pub start_end: StartEnd,
    /// The stamp datetime
    pub stamp: DateTime<Utc>,
    /// The creation datetime
    pub created: DateTime<Utc>,
    /// The last modified datetime
    pub last_modified: DateTime<Utc>,
    /// The recurrence ID if this is a recurrence override
    pub recurrence_id: Option<Dt>,
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
                |tz| tz.from_local_datetime(date_time).single().map(|dt| dt.with_timezone(&Utc)),
            )
        }
        CalendarDateTime::Floating(naive_dt) => Some(Utc.from_utc_datetime(naive_dt)),
    }
}

fn format_datetime_for_rrule(dt: &CalendarDateTime) -> String {
    match dt {
        CalendarDateTime::Utc(d) => d.format("%Y%m%dT%H%M%SZ").to_string(),
        CalendarDateTime::WithTimezone { date_time, tzid } => {
            tzid.parse::<chrono_tz::Tz>()
                .map_or_else(
                    |_| date_time.format("%Y%m%dT%H%M%SZ").to_string(),
                    |tz| {
                        tz.from_local_datetime(date_time)
                            .single()
                            .map_or_else(
                                || date_time.format("%Y%m%dT%H%M%SZ").to_string(),
                                |dt| dt.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ").to_string(),
                            )
                    },
                )
        }
        CalendarDateTime::Floating(d) => d.format("%Y%m%dT%H%M%S").to_string(),
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn load<T: TimeZone + Clone>(
    url: &str,
    start: NaiveDate,
    stop: NaiveDate,
    tz: T,
) -> Result<Vec<CalendarEntry>, Error> {
    let text = reqwest::get(url).await?.error_for_status()?.text().await?;
    let calendar = text.parse::<IcalCalendar>().map_err(|_| Error::Ical)?;

    let mut entries = Vec::new();
    let start_dt: DateTime<T> = naive_date_to_datetime(start, &tz);
    let stop_dt: DateTime<T> = naive_date_to_datetime(stop, &tz);
    let start_dt_utc = start_dt.with_timezone(&Utc);
    let stop_dt_utc = stop_dt.with_timezone(&Utc);

    for component in calendar.iter() {
        if let icalendar::CalendarComponent::Event(event) = component {
            if event.property_value("RECURRENCE-ID").is_some() {
                continue;
            }

            let event_start_opt = event.get_start();
            let event_end_opt = event.get_end();

            let dt_start_str = match event_start_opt.as_ref() {
                Some(DatePerhapsTime::DateTime(ref dt)) => format_datetime_for_rrule(dt),
                Some(DatePerhapsTime::Date(date)) => date.format("%Y%m%d").to_string(),
                None => "20190318T040000Z".to_string(),
            };

            let (duration, entry_start_dt) = match (event_start_opt, event_end_opt) {
                (Some(DatePerhapsTime::DateTime(s)), Some(DatePerhapsTime::DateTime(e))) => {
                    let s_utc = calendar_datetime_to_utc(&s).unwrap_or(start_dt_utc);
                    let e_utc = calendar_datetime_to_utc(&e).unwrap_or_else(|| start_dt_utc + Duration::hours(1));
                    (e_utc - s_utc, Some(DatePerhapsTime::DateTime(s)))
                }
                (Some(DatePerhapsTime::Date(s)), Some(DatePerhapsTime::Date(e))) => {
                    let dur = Duration::days((e - s).num_days());
                    (dur, Some(DatePerhapsTime::Date(s)))
                }
                _ => (Duration::hours(1), None),
            };

            if let Some(rrule_str) = event.property_value("RRULE").map(ToString::to_string) {
                if entry_start_dt.is_some() {
                    let rrule_full_str = format!("DTSTART:{dt_start_str}\nRRULE:{rrule_str}\n");
                    
                    if let Ok(rrule_set) = rrule_full_str.parse::<rrule::RRuleSet>() {
                        let result = rrule_set.all(100);
                        
                        for occurrence in result.dates {
                            let occurrence_utc = occurrence.with_timezone(&Utc);
                            if occurrence_utc < start_dt_utc || occurrence_utc >= stop_dt_utc {
                                continue;
                            }
                            let occurrence_end = occurrence_utc + duration;
                            
                            let summary = event.get_summary().map(ToString::to_string).unwrap_or_default();
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
                                transp: "OPAQUE".to_string(),
                                sequence: 0,
                                start_end: StartEnd::DateTime(occurrence_utc, occurrence_end),
                                stamp: Utc::now(),
                                created: Utc::now(),
                                last_modified: Utc::now(),
                                recurrence_id: None,
                            });
                        }
                    }
                }
            } else if let Some(s) = entry_start_dt {
                let s_utc = match s {
                    DatePerhapsTime::DateTime(dt) => calendar_datetime_to_utc(&dt).unwrap_or(start_dt_utc),
                    DatePerhapsTime::Date(date) => naive_date_to_datetime(date, &tz).with_timezone(&Utc),
                };
                if s_utc >= start_dt_utc && s_utc < stop_dt_utc {
                    let e = s_utc + duration;
                    let summary = event.get_summary().map(ToString::to_string).unwrap_or_default();
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
                        transp: "OPAQUE".to_string(),
                        sequence: 0,
                        start_end: StartEnd::DateTime(s_utc, e),
                        stamp: Utc::now(),
                        created: Utc::now(),
                        last_modified: Utc::now(),
                        recurrence_id: None,
                    });
                }
            }
        }
    }

    entries.sort_by_key(|e| match &e.start_end {
        StartEnd::DateTime(s, _) => *s,
        StartEnd::Date(s, _) => naive_date_to_datetime(*s, &Utc),
    });

    Ok(entries)
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
    RRule(#[from] rrule::RRuleError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Berlin;

    #[tokio::test]
    async fn test_calendar() {
        let c = load(
            "https://raw.githubusercontent.com/niccokunzmann/python-recurring-ical-events/refs/tags/v3.3.0/test/calendars/recurring_events_changed_duration.ics",
            NaiveDate::from_ymd_opt(2019, 3, 5).unwrap(),
            NaiveDate::from_ymd_opt(2019, 4, 1).unwrap(),
            Berlin,
        )
        .await
        .unwrap();
        assert!(c.len() == 7);
    }

    #[tokio::test]
    async fn test_calendar_stop_same_date() {
        let c = load(
            "https://raw.githubusercontent.com/niccokunzmann/python-recurring-ical-events/refs/tags/v3.3.0/test/calendars/recurring_events_changed_duration.ics",
            NaiveDate::from_ymd_opt(2019, 3, 18).unwrap(),
            NaiveDate::from_ymd_opt(2019, 3, 18).unwrap(),
            Berlin,
        )
        .await
        .unwrap();
        assert!(c.is_empty());
    }

    #[tokio::test]
    async fn test_calendar_stop_next_day() {
        let c = load(
            "https://raw.githubusercontent.com/niccokunzmann/python-recurring-ical-events/refs/tags/v3.3.0/test/calendars/recurring_events_changed_duration.ics",
            NaiveDate::from_ymd_opt(2019, 3, 18).unwrap(),
            NaiveDate::from_ymd_opt(2019, 3, 19).unwrap(),
            Berlin,
        )
        .await
        .unwrap();
        assert!(c.len() == 1);
    }
}
