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

#[derive(Debug)]
pub(crate) enum StartEnd {
    Date(NaiveDate, NaiveDate),
    DateTime(DateTime<Utc>, DateTime<Utc>),
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
    pub(crate) start_end: StartEnd,
    pub(crate) stamp: DateTime<Utc>,
    pub(crate) created: DateTime<Utc>,
    pub(crate) last_modified: DateTime<Utc>,
    pub(crate) recurrence_id: Option<DateTime<Utc>>,
}

// impl CalendarEntry {
//     /// Compare the start time of two calendar entries.
//     ///
//     /// Note if one sequence has a `NaiveDate` and the other has a `DateTime`, then the `DateTime` will be converted to a `NaiveDate`.
//     ///
//     /// Otherwise the comparison is done on the `NaiveDate` or `DateTime` directly.
//     #[must_use]
//     pub fn cmp_start_time(&self, other: &Self) -> std::cmp::Ordering {
//         match (&self.start_end, &other.start_end) {
//             (StartEnd::Date(start1, _), StartEnd::Date(start2, _)) => start1.cmp(start2),
//             (StartEnd::DateTime(start1, _), StartEnd::DateTime(start2, _)) => start1.cmp(start2),
//             (StartEnd::Date(start1, _), StartEnd::DateTime(start2, _)) => start1.cmp(&start2.date_naive()),
//             (StartEnd::DateTime(start1, _), StartEnd::Date(start2, _)) => start1.date_naive().cmp(start2),
//         }
//     }
// }

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