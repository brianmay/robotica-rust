use crate::{delays::rate_limit, InitState};

use super::Prices;
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
    datetime::{time_delta, utc_now},
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct DayState {
    start: DateTime<Utc>,
    end: DateTime<Utc>,

    #[serde(with = "robotica_common::datetime::with_time_delta")]
    cheap_power_for_day: TimeDelta,
    last_cheap_update: Option<DateTime<Utc>>,
    is_on: bool,
}

const CHEAP_TIME: TimeDelta = unsafe_time_delta!(hours: 3);

impl DayState {
    fn new<T: TimeZone>(now: &DateTime<Utc>, timezone: &T) -> Self {
        let (start_day, end_day) = get_cheap_day(now, timezone);
        Self {
            start: start_day,
            end: end_day,
            cheap_power_for_day: TimeDelta::zero(),
            last_cheap_update: None,
            is_on: false,
        }
    }

    pub fn save(&self, psr: &PersistentStateRow<Self>) {
        psr.save(self).unwrap_or_else(|err| {
            error!("Failed to save day state: {}", err);
        });
    }

    pub fn load<T: TimeZone>(
        psr: &PersistentStateRow<Self>,
        now: &DateTime<Utc>,
        timezone: &T,
    ) -> Self {
        psr.load().unwrap_or_else(|err| {
            error!("Failed to load day state, using defaults: {}", err);
            Self::new(now, timezone)
        })
    }

    fn prices_to_hot_water_request<T: TimeZone>(
        &mut self,
        prices: &Prices,
        now: DateTime<Utc>,
        cheap_time: TimeDelta,
        timezone: &T,
    ) -> Request {
        let (_start_day, end_time) = get_cheap_day(&now, timezone);

        // If the date has changed, reset the cheap power for the day.
        if now < self.start || now >= self.end {
            *self = Self::new(&now, timezone);
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

        let duration = cheap_time
            .checked_sub(&self.cheap_power_for_day)
            .unwrap_or_else(TimeDelta::zero);

        info!(
            "Cheap power for day: {}, time left: {}",
            time_delta::to_string(&self.cheap_power_for_day),
            time_delta::to_string(&duration),
        );

        let is_cheap =
            prices.should_power_now("hotwater", Some(duration), now, end_time, self.is_on);

        let current_price = prices.get_weighted_price(now);
        let threshold = if self.is_on { 14.0 } else { 12.0 };

        let should_be_on = match (is_cheap, current_price) {
            (true, _) => true,
            (false, Some(price)) if price < threshold => true,
            _ => false,
        };

        if should_be_on {
            self.last_cheap_update = Some(now);
            self.is_on = true;
            Request::Heat
        } else {
            self.last_cheap_update = None;
            self.is_on = false;
            Request::DoNotHeat
        }
    }
}

fn get_cheap_day<T: TimeZone>(now: &DateTime<Utc>, local: &T) -> (DateTime<Utc>, DateTime<Utc>) {
    let end_time: NaiveTime = NaiveTime::from_hms_opt(15, 0, 0).unwrap_or_default();
    let (start_day, end_day) = super::private::get_day(now, end_time, local);
    (start_day, end_day)
}

pub fn run(state: &InitState, rx: Receiver<Arc<Prices>>) -> Receiver<Request> {
    let (tx_out, rx_out) = create_pipe("amber/hot_water");
    let timezone = &Local;

    let psr = state
        .persistent_state_database
        .for_name::<DayState>("amber");

    let mut day = DayState::load(&psr, &utc_now(), timezone);

    // Send initial state.
    {
        let initial = if day.is_on {
            Request::Heat
        } else {
            Request::DoNotHeat
        };
        info!("Sending initial state: {:?}", initial);
        tx_out.try_send(initial);
    }

    spawn(async move {
        let mut s = rx.subscribe().await;

        while let Ok(prices) = s.recv().await {
            let cr = day.prices_to_hot_water_request(&prices, Utc::now(), CHEAP_TIME, timezone);
            info!("Sending request: {:?}", cr);
            tx_out.try_send(cr);
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

    use crate::amber::api::{
        ChannelType, IntervalType, PeriodType, PriceResponse, TariffInformation,
    };
    use chrono::FixedOffset;
    use robotica_common::unsafe_duration;
    use std::time::Duration;
    use test_log::test;

    use super::*;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    const INTERVAL: Duration = unsafe_duration!(minutes: 30);

    #[test]
    fn test_day_state_new() {
        let now = "2020-01-01T00:00:00Z".parse().unwrap();
        let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();
        let ds = DayState::new(&now, &timezone);
        assert_eq!(
            ds,
            DayState {
                start: dt("2019-12-31T04:00:00Z"),
                end: dt("2020-01-01T04:00:00Z"),
                cheap_power_for_day: TimeDelta::minutes(0),
                last_cheap_update: None,
                is_on: false,
            }
        );
    }

    #[test]
    fn test_prices_to_hot_water_request_1() {
        // Arrange
        use IntervalType::CurrentInterval;
        use IntervalType::ForecastInterval;
        let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();

        let tariff_information = TariffInformation {
            period: PeriodType::Peak,
            season: None,
            block: None,
            demand_window: None,
        };

        let pr = |start_time: DateTime<Utc>, price, interval_type| {
            let date = start_time.with_timezone(&timezone).date_naive();
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
        let mut ds = DayState {
            start: dt("2019-12-31T04:00:00Z"),
            end: dt("2020-01-01T04:00:00Z"),
            cheap_power_for_day: TimeDelta::zero(),
            last_cheap_update: Some(dt("2020-01-01T00:00:00Z")),
            is_on: false,
        };

        let prices = vec![
            pr(dt("2020-01-01T00:30:00Z"), 30.0, CurrentInterval),
            pr(dt("2020-01-01T01:00:00Z"), 30.0, ForecastInterval),
            pr(dt("2020-01-01T01:10:00Z"), 30.0, ForecastInterval),
            pr(dt("2020-01-01T01:30:00Z"), 40.0, ForecastInterval),
            pr(dt("2020-01-01T02:00:00Z"), 30.0, ForecastInterval),
            pr(dt("2020-01-01T02:30:00Z"), 30.0, ForecastInterval),
            pr(dt("2020-01-01T03:00:00Z"), 20.0, ForecastInterval),
            pr(dt("2020-01-01T03:30:00Z"), 20.0, ForecastInterval),
            pr(dt("2020-01-01T04:00:00Z"), 20.0, ForecastInterval),
            pr(dt("2020-01-01T04:30:00Z"), 30.0, ForecastInterval),
            pr(dt("2020-01-01T05:00:00Z"), 40.0, ForecastInterval),
            pr(dt("2020-01-01T05:30:00Z"), 20.0, ForecastInterval),
        ];

        let prices = Prices {
            list: prices,
            dt: now,
            interval: INTERVAL,
        };
        let delta = TimeDelta::minutes(120);

        // Act
        let request = ds.prices_to_hot_water_request(&prices, now, delta, &timezone);

        // Assert
        assert!(matches!(request, Request::Heat));
        assert_eq!(
            ds,
            DayState {
                start: dt("2019-12-31T04:00:00Z"),
                end: dt("2020-01-01T04:00:00Z"),
                cheap_power_for_day: TimeDelta::minutes(30),
                last_cheap_update: Some(dt("2020-01-01T00:30:00Z")),
                is_on: true,
            }
        );
    }

    #[test]
    fn test_prices_to_hot_water_request_2() {
        // Arrange
        use IntervalType::ActualInterval;
        use IntervalType::CurrentInterval;
        use IntervalType::ForecastInterval;
        let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();

        let tariff_information = TariffInformation {
            period: PeriodType::Peak,
            season: None,
            block: None,
            demand_window: None,
        };

        let pr = |start_time: DateTime<Utc>, price, interval_type| {
            let date = start_time.with_timezone(&timezone).date_naive();
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

        let prices = vec![
            pr(dt("2020-01-01T00:30:00Z"), 15.0, ActualInterval),
            pr(dt("2020-01-01T01:00:00Z"), 15.0, CurrentInterval),
            pr(dt("2020-01-01T01:30:00Z"), 15.0, ForecastInterval),
            pr(dt("2020-01-01T02:00:00Z"), 20.0, ForecastInterval),
            pr(dt("2020-01-01T02:30:00Z"), 20.0, ForecastInterval),
            pr(dt("2020-01-01T03:00:00Z"), 20.0, ForecastInterval),
            pr(dt("2020-01-01T03:30:00Z"), -30.0, ForecastInterval),
            pr(dt("2020-01-01T04:00:00Z"), -30.0, ForecastInterval),
            pr(dt("2020-01-01T04:30:00Z"), -30.0, ForecastInterval),
            pr(dt("2020-01-01T05:00:00Z"), 30.0, ForecastInterval),
            pr(dt("2020-01-01T05:30:00Z"), 40.0, ForecastInterval),
            pr(dt("2020-01-01T06:00:00Z"), 40.0, ForecastInterval),
        ];

        let now: DateTime<Utc> = dt("2020-01-01T01:15:00Z");
        let mut ds = DayState {
            start: dt("2019-12-31T04:00:00Z"),
            end: dt("2020-01-01T04:00:00Z"),
            cheap_power_for_day: TimeDelta::minutes(30),
            last_cheap_update: Some(dt("2020-01-01T00:30:00Z")),
            is_on: false,
        };

        let prices = Prices {
            list: prices,
            dt: now,
            interval: INTERVAL,
        };
        let delta = TimeDelta::minutes(120);

        // Act
        let request = ds.prices_to_hot_water_request(&prices, now, delta, &timezone);

        // Assert
        assert!(matches!(request, Request::Heat));
        assert_eq!(
            ds,
            DayState {
                start: dt("2019-12-31T04:00:00Z"),
                end: dt("2020-01-01T04:00:00Z"),
                cheap_power_for_day: TimeDelta::minutes(30 + 45),
                last_cheap_update: Some(now),
                is_on: true,
            }
        );
    }

    #[test]
    fn test_prices_to_hot_water_request_2_cheaper_after_wait() {
        // Arrange
        use IntervalType::ActualInterval;
        use IntervalType::CurrentInterval;
        use IntervalType::ForecastInterval;
        let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();

        let tariff_information = TariffInformation {
            period: PeriodType::Peak,
            season: None,
            block: None,
            demand_window: None,
        };

        let pr = |start_time: DateTime<Utc>, price, interval_type| {
            let date = start_time.with_timezone(&timezone).date_naive();
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

        let prices = vec![
            pr(dt("2020-01-01T00:30:00Z"), 15.0, ActualInterval),
            pr(dt("2020-01-01T01:00:00Z"), 15.0, CurrentInterval),
            pr(dt("2020-01-01T01:30:00Z"), 15.0, ForecastInterval),
            pr(dt("2020-01-01T02:00:00Z"), 20.0, ForecastInterval),
            pr(dt("2020-01-01T02:30:00Z"), 20.0, ForecastInterval),
            pr(dt("2020-01-01T03:00:00Z"), -30.0, ForecastInterval),
            pr(dt("2020-01-01T03:30:00Z"), -30.0, ForecastInterval),
            pr(dt("2020-01-01T04:00:00Z"), -30.0, ForecastInterval),
            pr(dt("2020-01-01T04:30:00Z"), -30.0, ForecastInterval),
            pr(dt("2020-01-01T05:00:00Z"), 30.0, ForecastInterval),
            pr(dt("2020-01-01T05:30:00Z"), 40.0, ForecastInterval),
            pr(dt("2020-01-01T06:00:00Z"), 40.0, ForecastInterval),
        ];

        let now: DateTime<Utc> = dt("2020-01-01T01:15:00Z");
        let mut ds = DayState {
            start: dt("2019-12-31T04:00:00Z"),
            end: dt("2020-01-01T04:00:00Z"),
            cheap_power_for_day: TimeDelta::minutes(30),
            last_cheap_update: Some(dt("2020-01-01T00:30:00Z")),
            is_on: false,
        };

        let prices = Prices {
            list: prices,
            dt: now,
            interval: INTERVAL,
        };
        let delta = TimeDelta::minutes(120);

        // Act
        let request = ds.prices_to_hot_water_request(&prices, now, delta, &timezone);

        // Assert
        assert!(matches!(request, Request::DoNotHeat));
        assert_eq!(
            ds,
            DayState {
                start: dt("2019-12-31T04:00:00Z"),
                end: dt("2020-01-01T04:00:00Z"),
                cheap_power_for_day: TimeDelta::minutes(30 + 45),
                last_cheap_update: None,
                is_on: false,
            }
        );
    }

    #[test]
    fn test_prices_to_hot_water_request_3_force_end_day() {
        // Arrange
        use IntervalType::ActualInterval;
        use IntervalType::CurrentInterval;
        use IntervalType::ForecastInterval;
        let timezone = FixedOffset::east_opt(11 * 60 * 60).unwrap();

        let tariff_information = TariffInformation {
            period: PeriodType::Peak,
            season: None,
            block: None,
            demand_window: None,
        };

        let pr = |start_time: DateTime<Utc>, price, interval_type| {
            let date = start_time.with_timezone(&timezone).date_naive();
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

        let prices = vec![
            pr(dt("2020-01-01T00:30:00Z"), 15.0, ActualInterval),
            pr(dt("2020-01-01T01:00:00Z"), 15.0, CurrentInterval),
            pr(dt("2020-01-01T01:30:00Z"), 15.0, ForecastInterval),
            pr(dt("2020-01-01T02:00:00Z"), 20.0, ForecastInterval),
            pr(dt("2020-01-01T02:30:00Z"), 15.0, ForecastInterval),
            pr(dt("2020-01-01T03:00:00Z"), 10.0, ForecastInterval),
            pr(dt("2020-01-01T03:30:00Z"), 5.0, ForecastInterval),
            pr(dt("2020-01-01T04:00:00Z"), 0.0, ForecastInterval),
            pr(dt("2020-01-01T04:30:00Z"), -5.0, ForecastInterval),
            pr(dt("2020-01-01T05:00:00Z"), 30.0, ForecastInterval),
            pr(dt("2020-01-01T05:30:00Z"), 40.0, ForecastInterval),
            pr(dt("2020-01-01T06:00:00Z"), 40.0, ForecastInterval),
        ];

        let now: DateTime<Utc> = dt("2020-01-01T02:00:00Z");
        let mut ds = DayState {
            start: dt("2019-12-31T04:00:00Z"),
            end: dt("2020-01-01T04:00:00Z"),
            cheap_power_for_day: TimeDelta::minutes(0),
            last_cheap_update: None,
            is_on: false,
        };

        let prices = Prices {
            list: prices,
            dt: now,
            interval: INTERVAL,
        };
        let delta = TimeDelta::minutes(120);

        // Act
        let request = ds.prices_to_hot_water_request(&prices, now, delta, &timezone);

        // Assert
        assert!(matches!(request, Request::Heat));
        assert_eq!(
            ds,
            DayState {
                start: dt("2019-12-31T04:00:00Z"),
                end: dt("2020-01-01T04:00:00Z"),
                cheap_power_for_day: TimeDelta::minutes(0),
                last_cheap_update: Some(now),
                is_on: true,
            }
        );
    }

    #[test]
    fn test_get_cheap_day() {
        let timezone = FixedOffset::east_opt(60 * 60 * 11).unwrap();
        let now = dt("2020-01-02T00:00:00Z");
        let (start, stop) = get_cheap_day(&now, &timezone);
        assert_eq!(start, dt("2020-01-01T04:00:00Z"));
        assert_eq!(stop, dt("2020-01-02T04:00:00Z"));
    }
}
