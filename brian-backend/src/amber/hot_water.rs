use crate::{delays::rate_limit, InitState};

use super::{
    api::{IntervalType, PriceResponse},
    Prices,
};
use chrono::{DateTime, Local, NaiveTime, TimeDelta, TimeZone, Utc};
use robotica_backend::{
    pipes::{
        stateful::{create_pipe, Receiver},
        Subscriber, Subscription,
    },
    services::persistent_state::PersistentStateRow,
    spawn,
};
use robotica_common::{
    datetime::{convert_date_time_to_utc_or_default, time_delta, utc_now},
    unsafe_time_delta,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Request {
    Heat,
    DoNotHeat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DayState {
    start: DateTime<Utc>,
    end: DateTime<Utc>,

    #[serde(with = "robotica_common::datetime::with_time_delta")]
    cheap_power_for_day: TimeDelta,
    last_cheap_update: Option<DateTime<Utc>>,
    cheapest_price: f32,
}

const CHEAP_TIME: TimeDelta = unsafe_time_delta!(hours: 2);
const ONE_DAY: TimeDelta = unsafe_time_delta!(days: 1);

impl DayState {
    fn new(now: &DateTime<Utc>) -> Self {
        let (start_day, end_day) = get_2hr_day(now);
        Self {
            start: start_day,
            end: end_day,
            cheap_power_for_day: TimeDelta::zero(),
            last_cheap_update: None,
            cheapest_price: 10.0,
        }
    }

    pub fn save(&self, psr: &PersistentStateRow<Self>) {
        psr.save(self).unwrap_or_else(|err| {
            error!("Failed to save day state: {}", err);
        });
    }

    pub fn load(psr: &PersistentStateRow<Self>, now: &DateTime<Utc>) -> Self {
        psr.load().unwrap_or_else(|err| {
            error!("Failed to load day state, using defaults: {}", err);
            Self::new(now)
        })
    }

    // FIXME: is this too complicated?
    #[allow(clippy::cognitive_complexity)]
    fn prices_to_hot_water_request(
        &mut self,
        prices: &Prices,
        now: DateTime<Utc>,
    ) -> Option<Request> {
        let Some(current_price) = prices.current(&now) else {
            error!("No current price found in prices: {prices:?}");
            return None;
        };

        let (start_day, end_day) = get_2hr_day(&now);

        // If the date has changed, reset the cheap power for the day.
        if now < self.start || now >= self.end {
            *self = Self::new(&now);
        };

        // Add recent time to total cheap_power_for_day
        if let Some(last_cheap_update) = &self.last_cheap_update {
            let duration = now - *last_cheap_update;
            info!(
                "Adding {:?} to cheap power for day {now:?} - {last_cheap_update:?}",
                duration
            );
            self.cheap_power_for_day += duration;
        }

        let interval_duration = prices.interval;
        // Something is seriously messed up if conversion from u64 to i64 fails.
        let interval_minutes: i64 = (interval_duration.as_secs() / 60).try_into().unwrap_or(30);

        let duration = CHEAP_TIME
            .checked_sub(&self.cheap_power_for_day)
            .unwrap_or_else(TimeDelta::zero);

        let number_of_intervals = divide_round_up(duration.num_minutes(), interval_minutes);
        let number_of_intervals: usize = number_of_intervals.try_into().unwrap_or_default();

        info!(
            "Cheap power for day: {}, time left: {}, number of intervals: {}/{}={}",
            time_delta::to_string(&self.cheap_power_for_day),
            time_delta::to_string(&duration),
            duration.num_minutes(),
            interval_minutes,
            number_of_intervals
        );

        self.cheapest_price =
            get_price_for_cheapest_period(&prices.list, number_of_intervals, &start_day, &end_day)
                .unwrap_or(self.cheapest_price);

        let is_cheap = current_price.per_kwh <= self.cheapest_price;
        info!(
            "Cheapest price: {cheapest_price:?} {is_cheap}",
            cheapest_price = self.cheapest_price
        );

        if is_cheap {
            self.last_cheap_update = Some(now);
            Some(Request::Heat)
        } else {
            self.last_cheap_update = None;
            Some(Request::DoNotHeat)
        }
    }
}

fn get_2hr_day(now: &DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let time_2hr_cheap: NaiveTime = NaiveTime::from_hms_opt(15, 0, 0).unwrap_or_default();
    let (start_day, end_day) = get_day(now, time_2hr_cheap, &Local);
    (start_day, end_day)
}

fn get_day<T: TimeZone>(
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

/// Divide two numbers and round up
const fn divide_round_up(dividend: i64, divisor: i64) -> i64 {
    (dividend + divisor - 1) / divisor
}

fn get_price_for_cheapest_period(
    prices: &[PriceResponse],
    number_of_intervals: usize,
    start_time: &DateTime<Utc>,
    end_time: &DateTime<Utc>,
) -> Option<f32> {
    if number_of_intervals == 0 {
        return None;
    }

    let mut prices: Vec<_> = prices
        .iter()
        .filter(|p| {
            p.start_time >= *start_time
                && p.start_time < *end_time
                && p.interval_type != IntervalType::ActualInterval
        })
        .map(|p| p.per_kwh)
        .collect();

    prices.sort_by(f32::total_cmp);
    // println!("Prices: {prices:?} {number_of_intervals}");

    prices
        .get(number_of_intervals - 1)
        .or_else(|| prices.last())
        .copied()
}

pub fn run(state: &InitState, rx: Receiver<Arc<Prices>>) -> Receiver<Request> {
    let (tx_out, rx_out) = create_pipe("amber/hot_water");

    let psr = state
        .persistent_state_database
        .for_name::<DayState>("amber");

    let mut day = DayState::load(&psr, &utc_now());

    spawn(async move {
        let mut s = rx.subscribe().await;

        while let Ok(prices) = s.recv().await {
            let cr = day.prices_to_hot_water_request(&prices, Utc::now());
            if let Some(cr) = cr {
                tx_out.try_send(cr);
            }
            day.save(&psr);
        }
    });

    rate_limit(
        "amber/hot_water/ratelimit",
        Duration::from_secs(300),
        rx_out,
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]
    use chrono::{FixedOffset, Local};
    use robotica_common::unsafe_duration;
    use std::time::Duration;

    use crate::amber::{
        api::{ChannelType, PeriodType, TariffInformation},
        PriceCategory,
    };

    use super::*;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    const INTERVAL: Duration = unsafe_duration!(minutes: 30);

    #[test]
    fn test_get_price_for_cheapest_period() {
        let tariff_information = TariffInformation {
            period: PeriodType::Peak,
            season: None,
            block: None,
            demand_window: None,
        };

        let pr = |start_time: DateTime<Utc>, price| {
            let date = start_time.with_timezone(&Local).date_naive();
            let end_time = start_time + INTERVAL;
            PriceResponse {
                date,
                start_time,
                end_time,
                per_kwh: price,
                spot_per_kwh: price,
                interval_type: IntervalType::CurrentInterval,
                renewables: 0.0,
                duration: 0,
                channel_type: ChannelType::General,
                estimate: Some(false),
                spike_status: "None".to_string(),
                tariff_information: tariff_information.clone(),
            }
        };

        let prices = vec![
            pr(dt("2020-01-01T00:30:00Z"), -10.0),
            pr(dt("2020-01-01T01:00:00Z"), 0.0),
            pr(dt("2020-01-01T01:30:00Z"), 10.0),
            pr(dt("2020-01-01T02:00:00Z"), 0.0),
            pr(dt("2020-01-01T02:30:00Z"), 0.0),
            pr(dt("2020-01-01T03:30:00Z"), -10.0),
            pr(dt("2020-01-01T04:00:00Z"), 0.0),
            pr(dt("2020-01-01T04:30:00Z"), 0.0),
            pr(dt("2020-01-01T05:00:00Z"), 10.0),
            pr(dt("2020-01-01T05:30:00Z"), -10.0),
            pr(dt("2020-01-01T06:00:00Z"), -10.0),
        ];

        let start_time: DateTime<Utc> = "2020-01-01T00:00:00Z".parse().unwrap();
        let end_time: DateTime<Utc> = "2020-01-01T06:30:00Z".parse().unwrap();
        assert_eq!(
            get_price_for_cheapest_period(&prices, 0, &start_time, &end_time),
            None
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 1, &start_time, &end_time),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 2, &start_time, &end_time),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 3, &start_time, &end_time),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 4, &start_time, &end_time),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 5, &start_time, &end_time),
            Some(0.0)
        );

        let start_time: DateTime<Utc> = dt("2020-01-01T00:00:00Z");
        let end_time: DateTime<Utc> = dt("2020-01-01T06:00:00Z");
        assert_eq!(
            get_price_for_cheapest_period(&prices, 0, &start_time, &end_time),
            None
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 1, &start_time, &end_time),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 2, &start_time, &end_time),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 3, &start_time, &end_time),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 4, &start_time, &end_time),
            Some(0.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 5, &start_time, &end_time),
            Some(-0.0)
        );
    }

    #[test]
    fn test_prices_to_hot_water_request() {
        use IntervalType::ActualInterval;
        use IntervalType::CurrentInterval;
        use IntervalType::ForecastInterval;

        let tariff_information = TariffInformation {
            period: PeriodType::Peak,
            season: None,
            block: None,
            demand_window: None,
        };

        let pr = |start_time: DateTime<Utc>, price, interval_type| {
            let date = start_time.with_timezone(&Local).date_naive();
            let end_time = start_time + INTERVAL;
            PriceResponse {
                date,
                start_time,
                end_time,
                per_kwh: price,
                spot_per_kwh: price,
                interval_type,
                renewables: 0.0,
                duration: 0,
                channel_type: ChannelType::General,
                estimate: Some(false),
                spike_status: "None".to_string(),
                tariff_information: tariff_information.clone(),
            }
        };

        let now = "2020-01-01T00:30:00Z".parse().unwrap();
        let mut ds = DayState::new(&now);

        let prices = vec![
            pr(dt("2020-01-01T00:30:00Z"), 0.0, CurrentInterval),
            pr(dt("2020-01-01T01:00:00Z"), 0.0, ForecastInterval),
            pr(dt("2020-01-01T01:10:00Z"), 0.0, ForecastInterval),
            pr(dt("2020-01-01T01:30:00Z"), 10.0, ForecastInterval),
            pr(dt("2020-01-01T02:00:00Z"), 0.0, ForecastInterval),
            pr(dt("2020-01-01T02:30:00Z"), 0.0, ForecastInterval),
            pr(dt("2020-01-01T03:30:00Z"), -10.0, ForecastInterval),
            pr(dt("2020-01-01T04:00:00Z"), -10.0, ForecastInterval),
            pr(dt("2020-01-01T04:30:00Z"), 0.0, ForecastInterval),
            pr(dt("2020-01-01T05:00:00Z"), 10.0, ForecastInterval),
            pr(dt("2020-01-01T05:30:00Z"), -10.0, ForecastInterval),
        ];

        let prices = Prices {
            list: prices,
            category: PriceCategory::SuperCheap,
            dt: now,
            interval: INTERVAL,
        };

        let request = ds.prices_to_hot_water_request(&prices, now).unwrap();
        assert!(matches!(request, Request::Heat));
        assert_eq!(ds.cheap_power_for_day, TimeDelta::zero());
        let cp = ds.last_cheap_update.unwrap();
        assert_eq!(cp, now);

        let prices = vec![
            pr(dt("2020-01-01T00:30:00Z"), 0.0, ActualInterval),
            pr(dt("2020-01-01T01:00:00Z"), 0.0, CurrentInterval),
            pr(dt("2020-01-01T01:30:00Z"), 0.0, ForecastInterval),
            pr(dt("2020-01-01T02:00:00Z"), 20.0, ForecastInterval),
            pr(dt("2020-01-01T02:30:00Z"), 20.0, ForecastInterval),
            pr(dt("2020-01-01T03:30:00Z"), -30.0, ForecastInterval),
            pr(dt("2020-01-01T04:00:00Z"), -30.0, ForecastInterval),
            pr(dt("2020-01-01T04:30:00Z"), -30.0, ForecastInterval),
            pr(dt("2020-01-01T05:00:00Z"), 30.0, ForecastInterval),
            pr(dt("2020-01-01T05:30:00Z"), 40.0, ForecastInterval),
            pr(dt("2020-01-01T06:00:00Z"), 40.0, ForecastInterval),
        ];

        let prices = Prices {
            list: prices,
            category: PriceCategory::SuperCheap,
            dt: now,
            interval: INTERVAL,
        };

        let now: DateTime<Utc> = dt("2020-01-01T01:15:00Z");
        let request = ds.prices_to_hot_water_request(&prices, now).unwrap();
        assert!(matches!(request, Request::DoNotHeat));
        assert_eq!(ds.cheap_power_for_day, TimeDelta::try_minutes(45).unwrap());
        let cp = ds.last_cheap_update;
        assert_eq!(cp, None);
    }

    #[test]
    fn test_divide_round_up() {
        assert_eq!(divide_round_up(0, 4), 0);
        assert_eq!(divide_round_up(1, 4), 1);
        assert_eq!(divide_round_up(2, 4), 1);
        assert_eq!(divide_round_up(3, 4), 1);
        assert_eq!(divide_round_up(4, 4), 1);
        assert_eq!(divide_round_up(5, 4), 2);
    }

    #[test]
    fn test_get_day() {
        let timezone = FixedOffset::east_opt(60 * 60 * 11).unwrap();
        {
            let time = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
            let now = dt("2020-01-02T00:00:00Z");
            let (start, stop) = get_day(&now, time, &timezone);
            assert_eq!(start, dt("2020-01-01T18:00:00Z"));
            assert_eq!(stop, dt("2020-01-02T18:00:00Z"));
        }

        {
            let time = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
            let now = dt("2020-01-02T17:59:59Z");
            let (start, stop) = get_day(&now, time, &timezone);
            assert_eq!(start, dt("2020-01-01T18:00:00Z"));
            assert_eq!(stop, dt("2020-01-02T18:00:00Z"));
        }

        {
            let time = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
            let now = "2020-01-02T18:00:00Z".parse().unwrap();
            let (start, stop) = get_day(&now, time, &timezone);
            assert_eq!(start, dt("2020-01-02T18:00:00Z"));
            assert_eq!(stop, dt("2020-01-03T18:00:00Z"));
        }

        {
            let time = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
            let now = "2020-01-02T18:00:01Z".parse().unwrap();
            let (start, stop) = get_day(&now, time, &timezone);
            assert_eq!(start, dt("2020-01-02T18:00:00Z"));
            assert_eq!(stop, dt("2020-01-03T18:00:00Z"));
        }
    }
}
