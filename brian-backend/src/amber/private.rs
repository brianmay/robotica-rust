use std::time::Duration;

use chrono::TimeDelta;

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

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

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
}
