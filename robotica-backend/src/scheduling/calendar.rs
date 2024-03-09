//! Provide ability to load from Google Calendar.

use chrono::{DateTime, NaiveDate, Utc};
use pyo3::prelude::*;
use pyo3::{FromPyObject, PyAny};

#[derive(Debug, FromPyObject)]
enum Dt {
    // DateTime must come first here.
    DateTime(DateTime<Utc>),
    Date(NaiveDate),
}

/// The start and end date/times of a calendar entry.
#[derive(Debug)]
pub enum StartEnd {
    /// This event is a daily event.
    Date(NaiveDate, NaiveDate),

    /// This event is not a daily event.
    DateTime(DateTime<Utc>, DateTime<Utc>),
}

/// A parsed calendar entry.
#[allow(dead_code)]
#[derive(Debug)]
pub struct CalendarEntry {
    /// The title of the event.
    pub summary: String,
    /// The description of the event.
    pub description: Option<String>,
    /// The location of the event.
    pub location: Option<String>,
    /// The unique id of the event.
    pub uid: String,
    /// The status of the event.
    pub status: Option<String>,
    /// The transparency of the event.
    pub transp: String,
    /// The sequence of the event.
    pub sequence: u8,
    /// The start and end date/times of the event.
    pub start_end: StartEnd,
    /// The stamp of the event.
    pub stamp: DateTime<Utc>,
    /// The creation time of the event.
    pub created: DateTime<Utc>,
    /// The last modified time of the event.
    pub last_modified: DateTime<Utc>,
    /// The recurrence id of the event.
    pub recurrence_id: Option<DateTime<Utc>>,
}

impl FromPyObject<'_> for CalendarEntry {
    fn extract(ob: &'_ PyAny) -> pyo3::PyResult<Self> {
        let start: Dt = ob.get_item("DTSTART")?.extract()?;
        let end: Dt = ob.get_item("DTEND")?.extract()?;

        let start_end = match (start, end) {
            (Dt::DateTime(start), Dt::DateTime(end)) => StartEnd::DateTime(start, end),
            (Dt::Date(start), Dt::Date(end)) => StartEnd::Date(start, end),
            (Dt::DateTime(_), Dt::Date(_)) => {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "DTSTART is a DateTime but DTEND is a Date",
                ))
            }
            (Dt::Date(_), Dt::DateTime(_)) => {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "DTSTART is a Date but DTEND is a DateTime",
                ))
            }
        };

        Ok(CalendarEntry {
            summary: ob.get_item("SUMMARY")?.extract()?,
            description: ob
                .get_item("DESCRIPTION")
                .map_or_else(|_| Ok(None), PyAny::extract)?,
            location: ob
                .get_item("LOCATION")
                .map_or_else(|_| Ok(None), PyAny::extract)?,
            uid: ob.get_item("UID")?.extract()?,
            status: ob
                .get_item("STATUS")
                .map_or_else(|_| Ok(None), PyAny::extract)?,
            transp: ob.get_item("TRANSP")?.extract()?,
            sequence: ob.get_item("SEQUENCE")?.extract()?,
            start_end,
            stamp: ob.get_item("DTSTAMP")?.extract()?,
            created: ob.get_item("CREATED")?.extract()?,
            last_modified: ob.get_item("LAST-MODIFIED")?.extract()?,
            recurrence_id: ob
                .get_item("RECURRENCE-ID")
                .map_or_else(|_| Ok(None), PyAny::extract)?,
        })
    }
}

pub(crate) type Calendar = Vec<CalendarEntry>;

pub(crate) fn load(url: &str, start: NaiveDate, stop: NaiveDate) -> Result<Calendar, PyErr> {
    #![allow(clippy::unwrap_used)]

    let py_app = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/python/robotica.py"));

    Python::with_gil(|py| {
        let app = PyModule::from_code(py, py_app, "robotica.py", "robotica")?;
        let args = (url, start, stop);
        let calendar: Calendar = app.getattr("read_calendar")?.call1(args)?.extract()?;
        Ok(calendar)
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_calendar() {
        let c = load(
            "http://tinyurl.com/y24m3r8f",
            NaiveDate::from_ymd_opt(2019, 3, 5).unwrap(),
            NaiveDate::from_ymd_opt(2019, 4, 1).unwrap(),
        )
        .unwrap();
        assert!(c.len() == 7);
        println!("{c:?}");
    }

    #[test]
    fn test_calendar_stop_same_date() {
        let c = load(
            "http://tinyurl.com/y24m3r8f",
            NaiveDate::from_ymd_opt(2019, 3, 18).unwrap(),
            NaiveDate::from_ymd_opt(2019, 3, 18).unwrap(),
        )
        .unwrap();
        assert!(c.is_empty());
        println!("{c:?}");
    }

    #[test]
    fn test_calendar_stop_next_day() {
        let c = load(
            "http://tinyurl.com/y24m3r8f",
            NaiveDate::from_ymd_opt(2019, 3, 18).unwrap(),
            NaiveDate::from_ymd_opt(2019, 3, 19).unwrap(),
        )
        .unwrap();
        assert!(c.len() == 1);
        println!("{c:?}");
    }
}
