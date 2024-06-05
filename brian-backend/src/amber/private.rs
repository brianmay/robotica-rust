use std::time::Duration;

use chrono::{DateTime, NaiveTime, TimeDelta, TimeZone, Utc};
use robotica_common::datetime::convert_date_time_to_utc_or_default;

use super::ONE_DAY;

pub fn time_delta_to_number_intervals(duration: TimeDelta, interval_duration: Duration) -> usize {
    // Something is seriously messed up if conversion from u64 to i64 fails.
    let interval_minutes: i64 = (interval_duration.as_secs() / 60).try_into().unwrap_or(30);

    let number_of_intervals = divide_round_up(duration.num_minutes(), interval_minutes);
    let number_of_intervals: usize = number_of_intervals.try_into().unwrap_or_default();

    number_of_intervals
}

/// Divide two numbers and round up
const fn divide_round_up(dividend: i64, divisor: i64) -> i64 {
    (dividend + divisor - 1) / divisor
}

pub fn get_day<T: TimeZone>(
    now: &DateTime<Utc>,
    time: NaiveTime,
    local: &T,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let today = now.with_timezone(local).date_naive();
    let tomorrow = today + ONE_DAY;
    let mut start_day = convert_date_time_to_utc_or_default(today, time, local);
    let mut end_day = convert_date_time_to_utc_or_default(tomorrow, time, local);
    if *now < start_day {
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
    use rstest::rstest;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    #[rstest]
    #[case(0, 4, 0)]
    #[case(1, 4, 1)]
    #[case(2, 4, 1)]
    #[case(3, 4, 1)]
    #[case(4, 4, 1)]
    #[case(5, 4, 2)]
    fn test_divide_round_up(#[case] a: i64, #[case] b: i64, #[case] expected: i64) {
        assert_eq!(expected, divide_round_up(a, b));
    }

    /// Test that the conversion from TimeDelta to number of intervals works as expected
    #[rstest]
    #[case(0, 4, 0)]
    #[case(1, 4, 1)]
    #[case(2, 4, 1)]
    #[case(3, 4, 1)]
    #[case(4, 4, 1)]
    #[case(5, 4, 2)]
    #[case(6, 4, 2)]
    #[case(7, 4, 2)]
    #[case(8, 4, 2)]
    #[case(9, 4, 3)]
    fn test_timedelta_to_number_intervals(#[case] a: i64, #[case] b: u64, #[case] expected: usize) {
        let duration = TimeDelta::minutes(a);
        let interval_duration = Duration::from_secs(b * 60);
        assert_eq!(
            expected,
            time_delta_to_number_intervals(duration, interval_duration)
        );
    }

    #[test]
    fn test_get_day_1() {
        let timezone = FixedOffset::east_opt(60 * 60 * 11).unwrap();
        let time = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
        let now = dt("2020-01-02T00:00:00Z");
        let (start, stop) = get_day(&now, time, &timezone);
        assert_eq!(start, dt("2020-01-01T18:00:00Z"));
        assert_eq!(stop, dt("2020-01-02T18:00:00Z"));
    }

    #[test]
    fn test_get_day_2() {
        let timezone = FixedOffset::east_opt(60 * 60 * 11).unwrap();
        let time = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
        let now = dt("2020-01-02T17:59:59Z");
        let (start, stop) = get_day(&now, time, &timezone);
        assert_eq!(start, dt("2020-01-01T18:00:00Z"));
        assert_eq!(stop, dt("2020-01-02T18:00:00Z"));
    }

    #[test]
    fn test_get_day_3() {
        let timezone = FixedOffset::east_opt(60 * 60 * 11).unwrap();
        let time = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
        let now = "2020-01-02T18:00:00Z".parse().unwrap();
        let (start, stop) = get_day(&now, time, &timezone);
        assert_eq!(start, dt("2020-01-02T18:00:00Z"));
        assert_eq!(stop, dt("2020-01-03T18:00:00Z"));
    }

    #[test]
    fn test_get_day_4() {
        let timezone = FixedOffset::east_opt(60 * 60 * 11).unwrap();
        let time = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
        let now = "2020-01-02T18:00:01Z".parse().unwrap();
        let (start, stop) = get_day(&now, time, &timezone);
        assert_eq!(start, dt("2020-01-02T18:00:00Z"));
        assert_eq!(stop, dt("2020-01-03T18:00:00Z"));
    }
}
