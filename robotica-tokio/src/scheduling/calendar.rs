//! Provide ability to load from Google Calendar.

use chrono::{DateTime, NaiveDate, Utc};
use pyo3::ffi::c_str;
use pyo3::prelude::*;
use pyo3::{FromPyObject, PyAny};

/// A value that could be a `DateTime` or a `Date`
#[derive(Debug, FromPyObject)]
pub enum Dt {
    // DateTime must come first here.
    /// A `DateTime` Value
    DateTime(DateTime<Utc>),
    /// A `Date` Value
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
    pub recurrence_id: Option<Dt>,
}

impl<'py> FromPyObject<'_, 'py> for CalendarEntry {
    type Error = pyo3::PyErr;

    fn extract(ob: Borrowed<'_, 'py, PyAny>) -> pyo3::PyResult<Self> {
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
                .map_or_else(|_| Ok(None), |f| f.extract().map(Some))?,
            location: ob
                .get_item("LOCATION")
                .map_or_else(|_| Ok(None), |f| f.extract().map(Some))?,
            uid: ob.get_item("UID")?.extract()?,
            status: ob
                .get_item("STATUS")
                .map_or_else(|_| Ok(None), |f| f.extract().map(Some))?,
            transp: ob.get_item("TRANSP")?.extract()?,
            sequence: ob.get_item("SEQUENCE")?.extract()?,
            start_end,
            stamp: ob.get_item("DTSTAMP")?.extract()?,
            created: ob.get_item("CREATED")?.extract()?,
            last_modified: ob.get_item("LAST-MODIFIED")?.extract()?,
            recurrence_id: ob
                .get_item("RECURRENCE-ID")
                .map_or_else(|_| Ok(None), |f| f.extract().map(Some))?,
        })
    }
}

pub(crate) type Calendar = Vec<CalendarEntry>;

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Python error: {0}")]
    Python(#[from] PyErr),

    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
}

pub(crate) async fn load(url: &str, start: NaiveDate, stop: NaiveDate) -> Result<Calendar, Error> {
    #![allow(clippy::unwrap_used)]

    let text = reqwest::get(url).await?.error_for_status()?.text().await?;

    let py_app = c_str!(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/python/robotica.py"
    )));

    Python::attach(|py| {
        let app = PyModule::from_code(py, py_app, c_str!("robotica.py"), c_str!("robotica"))?;
        let args = (text, start, stop);
        let calendar: Calendar = app.getattr("read_calendar")?.call1(args)?.extract()?;
        Ok(calendar)
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[tokio::test]
    async fn test_calendar() {
        let c = load(
            "https://raw.githubusercontent.com/niccokunzmann/python-recurring-ical-events/refs/tags/v3.3.0/test/calendars/recurring_events_changed_duration.ics",
            NaiveDate::from_ymd_opt(2019, 3, 5).unwrap(),
            NaiveDate::from_ymd_opt(2019, 4, 1).unwrap(),
        )
        .await
        .unwrap();
        assert!(c.len() == 7);
        println!("{c:?}");
    }

    #[tokio::test]
    async fn test_calendar_stop_same_date() {
        let c = load(
            "https://raw.githubusercontent.com/niccokunzmann/python-recurring-ical-events/refs/tags/v3.3.0/test/calendars/recurring_events_changed_duration.ics",
            NaiveDate::from_ymd_opt(2019, 3, 18).unwrap(),
            NaiveDate::from_ymd_opt(2019, 3, 18).unwrap(),
        )
        .await
        .unwrap();
        assert!(c.is_empty());
        println!("{c:?}");
    }

    #[tokio::test]
    async fn test_calendar_stop_next_day() {
        let c = load(
            "https://raw.githubusercontent.com/niccokunzmann/python-recurring-ical-events/refs/tags/v3.3.0/test/calendars/recurring_events_changed_duration.ics",
            NaiveDate::from_ymd_opt(2019, 3, 18).unwrap(),
            NaiveDate::from_ymd_opt(2019, 3, 19).unwrap(),
        )
        .await
        .unwrap();
        assert!(c.len() == 1);
        println!("{c:?}");
    }
}
