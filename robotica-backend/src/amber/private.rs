use chrono::{DateTime, NaiveTime, TimeZone, Utc};
use robotica_common::datetime::convert_date_time_to_utc_or_default;

use super::ONE_DAY;

pub fn get_day<T: TimeZone>(
    now: DateTime<Utc>,
    time: NaiveTime,
    local: &T,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let today = now.with_timezone(local).date_naive();
    let tomorrow = today + ONE_DAY;
    let mut start_day = convert_date_time_to_utc_or_default(today, time, local);
    let mut end_day = convert_date_time_to_utc_or_default(tomorrow, time, local);
    if now < start_day {
        start_day -= ONE_DAY;
        end_day -= ONE_DAY;
    }
    (start_day, end_day)
}

#[cfg(test)]
mod test {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use chrono::FixedOffset;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    #[test]
    fn test_get_day_1() {
        let timezone = FixedOffset::east_opt(60 * 60 * 11).unwrap();
        let time = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
        let now = dt("2020-01-02T00:00:00Z");
        let (start, stop) = get_day(now, time, &timezone);
        assert_eq!(start, dt("2020-01-01T18:00:00Z"));
        assert_eq!(stop, dt("2020-01-02T18:00:00Z"));
    }

    #[test]
    fn test_get_day_2() {
        let timezone = FixedOffset::east_opt(60 * 60 * 11).unwrap();
        let time = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
        let now = dt("2020-01-02T17:59:59Z");
        let (start, stop) = get_day(now, time, &timezone);
        assert_eq!(start, dt("2020-01-01T18:00:00Z"));
        assert_eq!(stop, dt("2020-01-02T18:00:00Z"));
    }

    #[test]
    fn test_get_day_3() {
        let timezone = FixedOffset::east_opt(60 * 60 * 11).unwrap();
        let time = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
        let now = "2020-01-02T18:00:00Z".parse().unwrap();
        let (start, stop) = get_day(now, time, &timezone);
        assert_eq!(start, dt("2020-01-02T18:00:00Z"));
        assert_eq!(stop, dt("2020-01-03T18:00:00Z"));
    }

    #[test]
    fn test_get_day_4() {
        let timezone = FixedOffset::east_opt(60 * 60 * 11).unwrap();
        let time = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
        let now = "2020-01-02T18:00:01Z".parse().unwrap();
        let (start, stop) = get_day(now, time, &timezone);
        assert_eq!(start, dt("2020-01-02T18:00:00Z"));
        assert_eq!(stop, dt("2020-01-03T18:00:00Z"));
    }
}
