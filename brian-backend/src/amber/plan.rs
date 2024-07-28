use std::{
    cmp::{max, min},
    fmt::Formatter,
};

use chrono::{TimeDelta, Utc};
use robotica_common::datetime::{datetime_to_string, time_delta};
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use super::Prices;

#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct Plan {
    kw: f32,
    start_time: chrono::DateTime<Utc>,
    end_time: chrono::DateTime<Utc>,
}

impl Plan {
    const fn new(
        kw: f32,
        start_time: chrono::DateTime<Utc>,
        end_time: chrono::DateTime<Utc>,
    ) -> Self {
        Self {
            kw,
            start_time,
            end_time,
        }
    }

    pub const fn new_nil(kw: f32, now: chrono::DateTime<Utc>) -> Self {
        Self {
            kw,
            start_time: now,
            end_time: now,
        }
    }

    #[cfg(test)]
    pub const fn new_test(
        kw: f32,
        start_time: chrono::DateTime<Utc>,
        end_time: chrono::DateTime<Utc>,
    ) -> Self {
        Self {
            kw,
            start_time,
            end_time,
        }
    }

    pub const fn get_kw(&self) -> f32 {
        self.kw
    }

    pub const fn get_start_time(&self) -> chrono::DateTime<Utc> {
        self.start_time
    }

    pub const fn get_end_time(&self) -> chrono::DateTime<Utc> {
        self.end_time
    }

    pub const fn with_start_time(self, start_time: chrono::DateTime<Utc>) -> Self {
        Self { start_time, ..self }
    }

    pub fn get_time_left(&self, now: chrono::DateTime<Utc>) -> TimeDelta {
        if now < self.start_time {
            // hasn't started yet
            self.get_timedelta()
        } else if now < self.end_time {
            // started but not finished
            self.end_time - now
        } else {
            // finished
            TimeDelta::zero()
        }
    }

    pub fn get_timedelta(&self) -> TimeDelta {
        self.end_time - self.start_time
    }

    pub fn is_current(&self, dt: chrono::DateTime<Utc>) -> bool {
        self.start_time <= dt && self.end_time > dt
    }

    pub fn get_forecast_cost(&self, now: chrono::DateTime<Utc>, prices: &Prices) -> Option<f32> {
        // We can't go back in time unfortunately.
        if self.end_time <= self.start_time {
            return None;
        }

        // Ensure now is within the requested period.
        let now = max(self.start_time, now);
        let now = min(self.end_time, now);
        let mut total = 0.0f32;

        let mut now = now;
        while now < self.end_time {
            let Some(p) = prices.find(now) else {
                debug!("Cannot find price for {now}");
                return None;
            };

            #[allow(clippy::cast_precision_loss)]
            let new_cost = {
                let end_time = min(self.end_time, p.end_time);
                // Calculate the remaining time for this period.
                let duration = end_time - now;

                p.per_kwh * self.kw * duration.num_seconds() as f32 / 3600.0
            };

            total += new_cost;
            now = if let Some(next) = prices.get_next_period(now) {
                next
            } else {
                debug!("Cannot find next period for {now}");
                return None;
            }
        }

        Some(total)
    }
}

impl std::fmt::Debug for Plan {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plan")
            .field("kw", &self.kw)
            .field("start_time", &datetime_to_string(&self.start_time))
            .field("end_time", &datetime_to_string(&self.end_time))
            .field("duration", &time_delta::to_string(&self.get_timedelta()))
            .finish()
    }
}

pub fn get_cheapest(
    kw: f32,
    start_search: chrono::DateTime<Utc>,
    end_search: chrono::DateTime<Utc>,
    required_duration: chrono::TimeDelta,
    prices: &Prices,
) -> Option<(Plan, f32)> {
    // Short circuit entire process if the search period is invalid.
    if end_search <= start_search {
        // Generate plan that will expire immediately because nothing to do.
        // Use lowest cost so this will override any current plan.
        let plan = Plan::new_nil(kw, start_search);
        return Some((plan, f32::MIN));
    }

    let interval = prices.interval;

    // Get the time of the next 30 minute interval from now
    #[allow(clippy::cast_possible_wrap)]
    let Some(next_interval) = prices.get_next_period(start_search) else {
        error!("Cannot find next interval for {start_search}");
        return None;
    };

    let now_time = {
        let end_time = min(start_search + required_duration, end_search);
        std::iter::once(Plan::new(kw, start_search, end_time))
    };

    let rest_times = (0..48) // 48 intervals in a day
        .map(|i| {
            let start_time = next_interval + interval * i;
            let end_time = min(start_time + required_duration, end_search);
            Plan::new(kw, start_time, end_time)
        })
        .take_while(|plan| plan.start_time < end_search);

    now_time
        .chain(rest_times)
        .filter_map(|plan| {
            let price = plan.get_forecast_cost(start_search, prices);
            // error!("Plan: {:?} Price: {:?}", plan, price);
            price.map(|price| {
                // We need the largest value, hence we get the negative duration.
                let duration = -plan.get_timedelta();
                let start_time = plan.start_time;
                (plan, duration, price, start_time)
            })
        })
        .min_by(|(_, da, a, ta), (_, db, b, tb)| {
            (da, a, ta)
                .partial_cmp(&(db, b, tb))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(plan, _, total_cost, _)| (plan, total_cost))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use crate::amber::api::{self, IntervalType};

    use super::*;
    use chrono::{DateTime, FixedOffset, TimeDelta};
    use float_cmp::assert_approx_eq;
    use robotica_common::unsafe_duration;
    use std::time::Duration;
    use tracing::debug;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    #[rstest::rstest]
    #[case(
        dt("2021-01-01T00:15:00Z"),
        TimeDelta::minutes(30),
        dt("2021-01-01T00:14:59Z"),
        false
    )]
    #[case(
        dt("2021-01-01T00:15:00Z"),
        TimeDelta::minutes(30),
        dt("2021-01-01T00:15:00Z"),
        true
    )]
    #[case(
        dt("2021-01-01T00:15:00Z"),
        TimeDelta::minutes(30),
        dt("2021-01-01T00:44:59Z"),
        true
    )]
    #[case(
        dt("2021-01-01T00:15:00Z"),
        TimeDelta::minutes(30),
        dt("2021-01-01T00:45:00Z"),
        false
    )]
    #[case(
        dt("2021-01-01T00:15:00Z"),
        TimeDelta::minutes(30),
        dt("2021-01-01T00:45:01Z"),
        false
    )]

    fn test_is_current(
        #[case] start_time: chrono::DateTime<Utc>,
        #[case] duration: chrono::TimeDelta,
        #[case] dt: chrono::DateTime<Utc>,
        #[case] expected: bool,
    ) {
        let plan = Plan::new(1.0, start_time, start_time + duration);
        assert_eq!(plan.is_current(dt), expected);
    }

    const INTERVAL: Duration = unsafe_duration!(minutes: 30);

    fn pr(
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        interval_type: IntervalType,
        cost: f32,
    ) -> api::PriceResponse {
        let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();

        api::PriceResponse {
            date: start_time.with_timezone(&timezone).date_naive(),
            start_time,
            end_time,
            per_kwh: cost,
            spot_per_kwh: 0.0,
            interval_type,
            renewables: 0.0,
            duration: 0,
            channel_type: api::ChannelType::General,
            estimate: Some(false),
            spike_status: "None".to_string(),
            tariff_information: api::TariffInformation {
                period: api::PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            },
        }
    }

    fn pr_list_constant(cost: f32) -> Vec<api::PriceResponse> {
        let time = dt("2020-01-01T00:00:00Z");
        #[allow(clippy::cast_possible_wrap)]
        let interval_minutes = (INTERVAL.as_secs() / 60) as i64;

        (0i8..48i8)
            .map(|i| {
                let i64 = i64::from(i);
                pr(
                    time + TimeDelta::minutes(i64 * interval_minutes),
                    time + TimeDelta::minutes((i64 + 1) * interval_minutes),
                    IntervalType::ForecastInterval,
                    cost,
                )
            })
            .collect::<Vec<api::PriceResponse>>()
    }

    fn pr_list_descending(cost: f32) -> Vec<api::PriceResponse> {
        let time = dt("2020-01-01T00:00:00Z");
        #[allow(clippy::cast_possible_wrap)]
        let interval_minutes = (INTERVAL.as_secs() / 60) as i64;

        (0i8..48i8)
            .map(|i| {
                let i64 = i64::from(i);
                let f32 = f32::from(i);
                pr(
                    time + TimeDelta::minutes(i64 * interval_minutes),
                    time + TimeDelta::minutes((i64 + 1) * interval_minutes),
                    IntervalType::ForecastInterval,
                    f32.mul_add(-0.5, cost),
                )
            })
            // .map(|p| {
            //     debug!("{:?}", p);
            //     p
            // })
            .collect::<Vec<api::PriceResponse>>()
    }

    #[test_log::test(rstest::rstest)]
    // test single period with different now times
    #[case(
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(30),
        dt("2020-01-01T00:00:00Z"),
        20.0 * 0.5 * 50.0 * 1.0
    )]
    #[rstest::rstest]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(30),
        dt("2020-01-01T00:00:00Z"),
        20.0 * 0.5 * 50.0 * 1.0
    )]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(30),
        dt("2020-01-01T00:15:00Z"),
        20.0 * 0.5 * 50.0 * 0.5
    )]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(30),
        dt("2020-01-01T00:18:00Z"),
        20.0 * 0.5 * 50.0 * 0.4
    )]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(30),
        dt("2020-01-01T00:30:00Z"),
        20.0 * 0.5 * 50.0 * 0.0
    )]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(30),
        dt("2020-01-01T01:30:00Z"),
        20.0 * 0.5 * 50.0 * 0.0
    )]
    // test twin period with different now times
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(60),
        dt("2020-01-01T00:00:00Z"),
        (20.0 * 0.5 * 50.0 * 1.0) + (20.0 * 0.5 * 49.5 * 1.0)
    )]
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(60),
        dt("2020-01-01T00:18:00Z"),
        (20.0 * 0.5 * 50.0 * 0.4) + (20.0 * 0.5 * 49.5 * 1.0)
    )]
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(60),
        dt("2020-01-01T00:30:00Z"),
        (20.0 * 0.5 * 50.0 * 0.0) + (20.0 * 0.5 * 49.5 * 1.0)
    )]
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(60),
        dt("2020-01-01T00:48:00Z"),
        (20.0 * 0.5 * 50.0 * 0.0) + (20.0 * 0.5 * 49.5 * 0.4)
    )]
    // Test start_time starts is start of second period
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:30:00Z"),
        TimeDelta::minutes(30),
        dt("2020-01-01T00:00:00Z"),
        (20.0 * 0.5 * 50.0 * 0.0) + (20.0 * 0.5 * 49.5 * 1.0)
    )]
    // Test end_time ends is end of first period
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        TimeDelta::minutes(30),
        dt("2020-01-01T00:00:00Z"),
        (20.0 * 0.5 * 50.0 * 1.0) + (20.0 * 0.5 * 49.5 * 0.0)
    )]
    // Test required period overlaps first and second periods
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:18:00Z"),
        TimeDelta::minutes(30),
        dt("2020-01-01T00:00:00Z"),
        (20.0 * 0.5 * 50.0 * 0.4) + (20.0 * 0.5 * 49.5 * 0.6)
    )]
    fn test_forecast_price(
        #[case] start_time: chrono::DateTime<Utc>,
        #[case] duration: chrono::TimeDelta,
        #[case] now: chrono::DateTime<Utc>,
        #[case] expected: f32,
    ) {
        let prices = Prices {
            list: pr_list_descending(50.0),
            interval: INTERVAL,
        };

        debug!("{start_time:?} {duration:?} {now:?} {expected:?}");
        let plan = Plan::new(20.0, start_time, start_time + duration);
        let price = plan.get_forecast_cost(now, &prices).unwrap();
        assert_approx_eq!(f32, price, expected);
    }

    #[test_log::test(rstest::rstest)]
    // test single period with different now times
    #[case(
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-02T01:30:00Z"),
        dt("2020-01-01T00:00:00Z"),
        20.0 * 0.5 * 50.0 * 1.0
    )]
    // We can't do negative durations!
    #[case(
        dt("2020-01-01T01:30:00Z"),
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T00:00:00Z"),
        20.0 * 0.5 * 50.0 * 1.0
    )]

    fn test_forecast_price_no_prices(
        #[case] start_time: chrono::DateTime<Utc>,
        #[case] end_time: chrono::DateTime<Utc>,
        #[case] now: chrono::DateTime<Utc>,
        #[case] expected: f32,
    ) {
        let prices = Prices {
            list: pr_list_descending(50.0),
            interval: INTERVAL,
        };

        debug!("{start_time:?} {end_time:?} {now:?} {expected:?}");
        let plan = Plan::new(20.0, start_time, end_time);
        let result = plan.get_forecast_cost(now, &prices);
        assert!(result.is_none());
    }

    #[test_log::test(rstest::rstest)]
    // Search scope one period only, look for one period, must return the same period.
    #[case(
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T00:30:00Z"),
        TimeDelta::minutes(30),
        20.0,
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T00:30:00Z"),
        20.0 * 0.5 * 50.0 * 1.0
    )]
    // Search scope two periods, look for one period, should return second.
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T01:00:00Z"),
        TimeDelta::minutes(30),
        20.0,
        dt("2020-01-01T00:30:00Z"),
        dt("2020-01-01T01:00:00Z"),
        (20.0 * 0.5 * 50.0 * 0.0) + (20.0 * 0.5 * 49.5 * 1.0)
    )]
    // Search scope two periods, look for two periods, should return both.
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T01:00:00Z"),
        TimeDelta::minutes(60),
        20.0,
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T01:00:00Z"),
        (20.0 * 0.5 * 50.0 * 1.0) + (20.0 * 0.5 * 49.5 * 1.0)
    )]
    // Search scope two periods, look for four periods, should return only 2.
    #[allow(clippy::suboptimal_flops)]
    #[case(
            dt("2020-01-01T00:00:00Z"),
            dt("2020-01-01T01:00:00Z"),
            TimeDelta::minutes(120),
            20.0,
            dt("2020-01-01T00:00:00Z"),
            dt("2020-01-01T01:00:00Z"),
            (20.0 * 0.5 * 50.0 * 1.0) + (20.0 * 0.5 * 49.5 * 1.0)
        )]
    // Search scope two periods but late start, look for two periods.
    // This also tests that the end time is not later then the search end time.
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:18:00Z"),
        dt("2020-01-01T01:00:00Z"),
        TimeDelta::minutes(60),
        20.0,
        dt("2020-01-01T00:18:00Z"),
        dt("2020-01-01T01:00:00Z"),
        (20.0 * 0.5 * 50.0 * 0.4) + (20.0 * 0.5 * 49.5 * 1.0)
    )]
    fn test_get_cheapest_plan(
        #[case] start_search: chrono::DateTime<Utc>,
        #[case] end_search: chrono::DateTime<Utc>,
        #[case] required_duration: chrono::TimeDelta,
        #[case] kw: f32,
        #[case] expected_start_time: chrono::DateTime<Utc>,
        #[case] expected_end_time: chrono::DateTime<Utc>,
        #[case] expected_price: f32,
    ) {
        let prices = Prices {
            list: pr_list_descending(50.0),
            interval: INTERVAL,
        };

        let (plan, cost) =
            get_cheapest(kw, start_search, end_search, required_duration, &prices).unwrap();
        assert_approx_eq!(f32, plan.kw, kw);
        assert_eq!(plan.start_time, expected_start_time);
        assert_eq!(plan.end_time, expected_end_time);
        assert!(plan.start_time >= start_search);
        assert!(plan.end_time <= end_search);
        assert_approx_eq!(f32, cost, expected_price);
    }

    // Wew should get earliest period that is available.
    #[test_log::test(rstest::rstest)]
    // Search scope one period only, look for one period, must return the same period.
    #[case(
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T00:30:00Z"),
        TimeDelta::minutes(30),
        20.0,
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T00:30:00Z"),
        20.0 * 0.5 * 50.0 * 1.0
    )]
    // Search scope two periods, look for one period, should return second.
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T01:00:00Z"),
        TimeDelta::minutes(30),
        20.0,
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T00:30:00Z"),
        20.0 * 0.5 * 50.0 * 1.0
    )]
    // Search scope two periods, look for two periods, should return both.
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T01:00:00Z"),
        TimeDelta::minutes(60),
        20.0,
        dt("2020-01-01T00:00:00Z"),
        dt("2020-01-01T01:00:00Z"),
        (20.0 * 0.5 * 50.0 * 1.0) + (20.0 * 0.5 * 50.0 * 1.0)
    )]
    // Search scope two periods, look for four periods, should return only 2.
    #[allow(clippy::suboptimal_flops)]
    #[case(
            dt("2020-01-01T00:00:00Z"),
            dt("2020-01-01T01:00:00Z"),
            TimeDelta::minutes(120),
            20.0,
            dt("2020-01-01T00:00:00Z"),
            dt("2020-01-01T01:00:00Z"),
            (20.0 * 0.5 * 50.0 * 1.0) + (20.0 * 0.5 * 50.0 * 1.0)
        )]
    // Search scope two periods but late start, look for two periods.
    // This also tests that the end time is not later then the search end time.
    #[allow(clippy::suboptimal_flops)]
    #[case(
        dt("2020-01-01T00:18:00Z"),
        dt("2020-01-01T01:00:00Z"),
        TimeDelta::minutes(60),
        20.0,
        dt("2020-01-01T00:18:00Z"),
        dt("2020-01-01T01:00:00Z"),
        (20.0 * 0.5 * 50.0 * 0.4) + (20.0 * 0.5 * 50.0 * 1.0)
    )]
    fn test_get_cheapest_plan_many_options(
        #[case] start_search: chrono::DateTime<Utc>,
        #[case] end_search: chrono::DateTime<Utc>,
        #[case] required_duration: chrono::TimeDelta,
        #[case] kw: f32,
        #[case] expected_start_time: chrono::DateTime<Utc>,
        #[case] expected_end_time: chrono::DateTime<Utc>,
        #[case] expected_price: f32,
    ) {
        let prices = Prices {
            list: pr_list_constant(50.0),
            interval: INTERVAL,
        };

        let (plan, cost) =
            get_cheapest(kw, start_search, end_search, required_duration, &prices).unwrap();
        assert_approx_eq!(f32, plan.kw, kw);
        assert_eq!(plan.start_time, expected_start_time);
        assert_eq!(plan.end_time, expected_end_time);
        assert!(plan.start_time >= start_search);
        assert!(plan.end_time <= end_search);
        assert_approx_eq!(f32, cost, expected_price);
    }

    #[rstest::rstest]
    #[case(
        dt("2020-01-31T00:00:00Z"),
        dt("2020-01-31T00:30:00Z"),
        TimeDelta::minutes(30),
        20.0
    )]
    fn test_get_cheapest_plan_no_prices(
        #[case] start_search: chrono::DateTime<Utc>,
        #[case] end_search: chrono::DateTime<Utc>,
        #[case] required_duration: chrono::TimeDelta,
        #[case] kw: f32,
    ) {
        let prices = Prices {
            list: pr_list_descending(50.0),
            interval: INTERVAL,
        };

        let result = get_cheapest(kw, start_search, end_search, required_duration, &prices);
        assert!(result.is_none());
    }

    #[rstest::rstest]
    #[case(
        dt("2020-01-31T00:30:00Z"),
        dt("2020-01-31T00:00:00Z"),
        TimeDelta::minutes(30),
        20.0
    )]
    #[case(
        dt("2020-01-31T00:30:00Z"),
        dt("2020-01-31T00:30:00Z"),
        TimeDelta::minutes(30),
        20.0
    )]
    fn test_get_cheapest_plan_nil(
        #[case] start_search: chrono::DateTime<Utc>,
        #[case] end_search: chrono::DateTime<Utc>,
        #[case] required_duration: chrono::TimeDelta,
        #[case] kw: f32,
    ) {
        let prices = Prices {
            list: pr_list_descending(50.0),
            interval: INTERVAL,
        };

        let (plan, cost) =
            get_cheapest(kw, start_search, end_search, required_duration, &prices).unwrap();

        assert_approx_eq!(f32, plan.kw, kw);
        assert_eq!(plan.start_time, start_search);
        assert_eq!(plan.end_time, start_search);
        assert_approx_eq!(f32, cost, f32::MIN);
    }
}
