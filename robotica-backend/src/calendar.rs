//! Provide ability to load from Google Calendar.

use chrono::{DateTime, NaiveDate, Utc};
use pyo3::prelude::*;
use pyo3::{FromPyObject, PyAny};

#[derive(Debug, FromPyObject)]
pub(crate) enum Dt {
    // DateTime must come first here.
    DateTime(DateTime<Utc>),
    Date(NaiveDate),
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct CalendarEntry {
    pub(crate) summary: String,
    pub(crate) description: Option<String>,
    pub(crate) location: Option<String>,
    pub(crate) uid: String,
    pub(crate) status: Option<String>,
    pub(crate) transp: String,
    pub(crate) sequence: u8,
    pub(crate) start: Dt,
    pub(crate) end: Dt,
    pub(crate) stamp: DateTime<Utc>,
    pub(crate) created: DateTime<Utc>,
    pub(crate) last_modified: DateTime<Utc>,
    pub(crate) recurrence_id: Option<DateTime<Utc>>,
}

impl FromPyObject<'_> for CalendarEntry {
    fn extract(ob: &'_ PyAny) -> pyo3::PyResult<Self> {
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
            start: ob.get_item("DTSTART")?.extract()?,
            end: ob.get_item("DTEND")?.extract()?,
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
        let app = PyModule::from_code(py, py_app, "robotica.py", "robotica").unwrap();
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
}
