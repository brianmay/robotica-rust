//! Get information from Amber electricity supplier

use chrono::{FixedOffset, Local, TimeZone, Utc};
use influxdb::InfluxDbWriteable;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::time::{interval, sleep_until, Instant, MissedTickBehavior};
use tracing::{debug, error, info};

use robotica_backend::{
    entities::{self, StatelessReceiver},
    get_env, is_debug_mode,
    services::persistent_state::PersistentStateRow,
    spawn, EnvironmentError,
};
use robotica_common::datetime::{
    convert_date_time_to_utc_or_default, duration_from_hms, utc_now, Date, DateTime, Duration, Time,
};

use crate::State;

/// Error when starting the Amber service
#[derive(Error, Debug)]
pub enum Error {
    /// Environment variable not found
    #[error("Environment variable error: {0}")]
    Environment(#[from] EnvironmentError),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

struct Config {
    token: String,
    site_id: String,
    influx_url: String,
    influx_database: String,
}

#[derive(InfluxDbWriteable)]
struct PriceReading {
    duration: u16,
    per_kwh: f32,
    renewables: f32,
    time: chrono::DateTime<Utc>,
    interval_type: IntervalType,
}

#[derive(InfluxDbWriteable)]
struct PriceSummaryReading {
    is_cheap_2hr: bool,
    per_kwh: f32,
    time: chrono::DateTime<Utc>,
}

#[derive(InfluxDbWriteable)]
struct UsageReading {
    duration: u16,
    per_kwh: f32,
    renewables: f32,
    kwh: f32,
    cost: f32,
    time: chrono::DateTime<Utc>,
}

const HOURS_TO_SECONDS: u16 = 3600;
const fn hours(num: u16) -> u16 {
    num * HOURS_TO_SECONDS
}

/// Get the current electricity price from Amber
///
/// # Errors
///
/// Returns an `AmberError` if the required environment variables are not set.
///
pub fn run(state: &State) -> Result<StatelessReceiver<PriceSummary>, Error> {
    let token = get_env("AMBER_TOKEN")?;
    let site_id = get_env("AMBER_SITE_ID")?;
    let influx_url = get_env("INFLUXDB_URL")?;
    let influx_database = get_env("INFLUXDB_DATABASE")?;
    let config = Config {
        token,
        site_id,
        influx_url,
        influx_database,
    };

    let (tx, rx) = entities::create_stateless_entity("amber_summary");

    let psr = state
        .persistent_state_database
        .for_name::<DayState>("amber");

    let nem_timezone = FixedOffset::east_opt(hours(10).into())
        .ok_or_else(|| Error::Internal("Failed to create NEM timezone".to_string()))?;

    spawn(async move {
        // if is_debug_mode() {
        //     let start_date = Date::from_ymd(2022, 1, 1);
        //     let stop_date = Date::from_ymd(2022, 3, 1);
        //     // process_prices(&config, start_date, stop_date).await;
        //     process_usage(&config, start_date, stop_date).await;
        //     println!("------------------- done -------------------");
        // }
        let mut pp = PriceProcessor::load(&psr, &utc_now());
        // Update prices maximum every 5 minutes
        let mut price_instant = Instant::now() + tokio::time::Duration::from_secs(0);

        // Update usage once an hour
        let mut usage_interval = interval(tokio::time::Duration::from_secs(hours(1).into()));
        usage_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = sleep_until(price_instant) => {
                    let now = utc_now();
                    let today = now.with_timezone(&nem_timezone).date_naive();
                    let yesterday = today - Duration::days(1);
                    let tomorrow = today + Duration::days(1);

                    // Get prices for the current interval.
                    let prices = get_prices(&config, yesterday, tomorrow).await;

                    // Process the results.
                    let next_delay = match prices {
                        Ok(prices) => {
                            // Update the summary.
                            let summary = pp.prices_to_summary(&now, &prices);
                            pp.save(&psr);

                            let update_time = summary.next_update;

                            // Write the prices to influxdb and send
                            prices_to_influxdb(&config, &prices, &summary).await;
                            tx.try_send(summary);

                            // Add margin to allow time for Amber to update.
                            let update_time = update_time + Duration::seconds(5);

                            // How long to the current interval expires?
                            let now = utc_now();
                            let duration: Duration = update_time - now;
                            info!("Next price update: {update_time:?} in {duration}");

                            // Ensure we update prices at least once once every 5 minutes.
                            let max_duration = Duration::minutes(5);
                            let min_duration = Duration::seconds(30);
                            duration.clamp(min_duration, max_duration)
                        }
                        Err(err) => {
                            error!("Failed to get prices: {}", err);
                            // If we failed to get prices, try again in 1 minute
                            Duration::minutes(1)
                        }
                    };

                    // Schedule the next update
                    info!("Next poll in {}", next_delay);
                    let next_delay: std::time::Duration = next_delay.to_std().unwrap_or(std::time::Duration::from_secs(300));
                    price_instant = Instant::now() + next_delay;
                }
                _ = usage_interval.tick() => {
                    // Update the amber usage once an hour.
                    let now = utc_now();
                    let today = now.with_timezone(&nem_timezone).date_naive();
                    let yesterday = today - Duration::days(1);
                    let tomorrow = today + Duration::days(1);
                    process_usage(&config, yesterday, tomorrow).await;
                }
            }
        }
    });

    Ok(rx)
}

#[allow(clippy::enum_variant_names)]
#[derive(Deserialize, Debug, Copy, Clone, Eq, PartialEq)]
enum IntervalType {
    ActualInterval,
    ForecastInterval,
    CurrentInterval,
}

impl From<IntervalType> for influxdb::Type {
    fn from(interval_type: IntervalType) -> Self {
        let v = match interval_type {
            IntervalType::ActualInterval => "actual",
            IntervalType::ForecastInterval => "forecast",
            IntervalType::CurrentInterval => "current",
        };
        Self::Text(v.to_string())
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
enum Quality {
    Estimated,
    Billable,
}

#[allow(clippy::enum_variant_names)]
#[derive(Deserialize, Debug)]
enum UsageType {
    Usage,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
enum ChannelType {
    General,
    ControlledLoad,
    FeedIn,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
enum PeriodType {
    OffPeak,
    Shoulder,
    SolarSponge,
    Peak,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
enum SeasonType {
    Default,
    Summer,
    Autumn,
    Winter,
    Spring,
    NonSummer,
    Holiday,
    Weekend,
    WeekendHoliday,
    Weekday,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct TariffInformation {
    period: PeriodType,
    season: Option<SeasonType>,
    block: Option<u32>,
    demand_window: Option<bool>,
}

/// Amber price response
#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PriceResponse {
    #[serde(rename = "type")]
    interval_type: IntervalType,
    duration: u16,
    spot_per_kwh: f32,
    per_kwh: f32,
    date: Date,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    renewables: f32,
    channel_type: ChannelType,
    tariff_information: TariffInformation,
    spike_status: String,
    estimate: Option<bool>,
}

/// Amber usage response
#[allow(dead_code)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsageResponse {
    #[serde(rename = "type")]
    usage_type: UsageType,
    duration: u16,
    spot_per_kwh: f32,
    per_kwh: f32,
    date: Date,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    renewables: f32,
    channel_type: ChannelType,
    tariff_information: TariffInformation,
    spike_status: String,
    channel_identifier: String,
    kwh: f32,
    quality: Quality,
    cost: f32,
}

async fn get_prices(
    config: &Config,
    start_date: Date,
    end_date: Date,
) -> Result<Vec<PriceResponse>, reqwest::Error> {
    let url = format!(
        "https://api.amber.com.au/v1/sites/{}/prices",
        config.site_id
    );

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("accept", "application/json")
        .header("authorization", format!("Bearer {}", config.token))
        .query(&[
            ("startDate", start_date.to_string()),
            ("endDate", end_date.to_string()),
        ])
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;

    response.json().await
}

async fn get_usage(
    config: &Config,
    start_date: Date,
    end_date: Date,
) -> Result<Vec<UsageResponse>, reqwest::Error> {
    let url = format!("https://api.amber.com.au/v1/sites/{}/usage", config.site_id);

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("accept", "application/json")
        .header("authorization", format!("Bearer {}", config.token))
        .query(&[
            ("startDate", start_date.to_string()),
            ("endDate", end_date.to_string()),
        ])
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;

    response.json().await
}

#[derive(Copy, Debug, Clone, Eq, PartialEq)]
pub enum PriceCategory {
    SuperCheap,
    Cheap,
    Normal,
    Expensive,
}
#[derive(Debug, Clone)]
pub struct PriceSummary {
    pub category: PriceCategory,
    pub is_cheap_2hr: bool,
    pub per_kwh: f32,
    pub next_update: DateTime<Utc>,
}

fn prices_to_category(prices: &[PriceResponse], category: Option<PriceCategory>) -> PriceCategory {
    let new_category = prices
        .iter()
        .find(|p| p.interval_type == IntervalType::CurrentInterval)
        .map_or(PriceCategory::Normal, |price| {
            get_price_category(category, price.per_kwh)
        });

    new_category
}

fn get_price_category(category: Option<PriceCategory>, price: f32) -> PriceCategory {
    let mut c = category.unwrap_or(PriceCategory::Normal);

    let under = |c: PriceCategory, threshold: f32, new_category: PriceCategory| {
        if price < threshold {
            new_category
        } else {
            c
        }
    };
    let over = |c: PriceCategory, threshold: f32, new_category: PriceCategory| {
        if price > threshold {
            new_category
        } else {
            c
        }
    };

    match c {
        PriceCategory::SuperCheap => {
            c = over(c, 11.0, PriceCategory::Cheap);
            c = over(c, 16.0, PriceCategory::Normal);
            c = over(c, 31.0, PriceCategory::Expensive);
        }
        PriceCategory::Cheap => {
            c = over(c, 16.0, PriceCategory::Normal);
            c = over(c, 31.0, PriceCategory::Expensive);
            c = under(c, 9.0, PriceCategory::SuperCheap);
        }
        PriceCategory::Normal => {
            c = over(c, 31.0, PriceCategory::Expensive);
            c = under(c, 14.0, PriceCategory::Cheap);
            c = under(c, 9.0, PriceCategory::SuperCheap);
        }
        PriceCategory::Expensive => {
            c = under(c, 29.0, PriceCategory::Normal);
            c = under(c, 14.0, PriceCategory::Cheap);
            c = under(c, 9.0, PriceCategory::SuperCheap);
        }
    }

    c
}

async fn prices_to_influxdb(config: &Config, prices: &[PriceResponse], summary: &PriceSummary) {
    let client = influxdb::Client::new(&config.influx_url, &config.influx_database);

    if is_debug_mode() {
        debug!("Skipping writing prices to influxdb in debug mode");
        return;
    }

    for data in prices {
        let reading = PriceReading {
            duration: data.duration,
            per_kwh: data.per_kwh,
            renewables: data.renewables,
            time: data.start_time,
            interval_type: data.interval_type,
        }
        .into_query("amber/price");

        if let Err(e) = client.query(&reading).await {
            error!("Failed to write to influxdb: {}", e);
        }
    }

    let reading = PriceSummaryReading {
        is_cheap_2hr: summary.is_cheap_2hr,
        per_kwh: summary.per_kwh,
        time: Utc::now(),
    }
    .into_query("amber/price_summary");

    if let Err(e) = client.query(&reading).await {
        error!("Failed to write to influxdb: {}", e);
    }
}

async fn process_usage(config: &Config, start_date: Date, end_date: Date) {
    let usage = get_usage(config, start_date, end_date).await;
    match usage {
        Ok(usage) => {
            let client = influxdb::Client::new(&config.influx_url, &config.influx_database);

            if is_debug_mode() {
                debug!("Skipping writing usage to influxdb in debug mode");
                return;
            }

            for data in usage {
                let name = format!("amber/usage/{}", data.channel_identifier);
                let reading = UsageReading {
                    duration: data.duration,
                    per_kwh: data.per_kwh,
                    renewables: data.renewables,
                    kwh: data.kwh,
                    cost: data.cost,
                    time: data.start_time,
                }
                .into_query(name);

                if let Err(e) = client.query(&reading).await {
                    error!("Failed to write to influxdb: {}", e);
                }
            }
        }
        Err(e) => {
            error!("Failed to get usage: {}", e);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DayState {
    start: DateTime<Utc>,
    end: DateTime<Utc>,

    #[serde(with = "robotica_common::datetime::with_duration")]
    cheap_power_for_day: Duration,
    last_cheap_update: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct PriceProcessor {
    day: DayState,
    category: Option<PriceCategory>,
}

impl PriceProcessor {
    #[cfg(test)]
    pub fn new(now: &DateTime<Utc>) -> Self {
        let day_state = new_day_state(now);
        Self {
            day: day_state,
            category: None,
        }
    }

    pub fn save(&self, psr: &PersistentStateRow<DayState>) {
        psr.save(&self.day).unwrap_or_else(|err| {
            error!("Failed to save day state: {}", err);
        });
    }

    pub fn load(psr: &PersistentStateRow<DayState>, now: &DateTime<Utc>) -> Self {
        let day = psr.load().unwrap_or_else(|err| {
            error!("Failed to load day state, using defaults: {}", err);
            new_day_state(now)
        });

        Self {
            day,
            category: None,
        }
    }

    pub fn prices_to_summary(
        &mut self,
        now: &DateTime<Utc>,
        prices: &[PriceResponse],
    ) -> PriceSummary {
        let Some(current_price) = prices
            .iter()
            .find(|p| p.interval_type == IntervalType::CurrentInterval)
            else {
                error!("No current price found in prices: {prices:?}");
                return PriceSummary{
                    is_cheap_2hr: false,
                    per_kwh: 100.0,
                    next_update: *now + Duration::seconds(30),
                    category: PriceCategory::Expensive
                }
            };

        let (start_day, end_day) = get_2hr_day(now);

        let mut ds = if *now < self.day.start || *now >= self.day.end {
            new_day_state(now)
        } else {
            self.day.clone()
        };

        if let Some(last_cheap_update) = &ds.last_cheap_update {
            let duration = *now - *last_cheap_update;
            // println!(
            //     "Adding {:?} to cheap power for day {now:?} - {current_cheap_update:?}",
            //     duration
            // );
            ds.cheap_power_for_day = ds.cheap_power_for_day + duration;
        }

        // println!("Cheap power for day: {:?}", self.cheap_power_for_day);

        // let start_time = convert_date_time_to_utc(today, Time::new(0, 0, 0), &Local).unwrap();
        // let end_time =
        //     convert_date_time_to_utc(today + Duration::days(1), Time::new(0, 0, 0), &Local)
        //         .unwrap();

        let interval_duration = Duration::minutes(30);
        let duration = Duration::hours(2)
            .checked_sub(&ds.cheap_power_for_day)
            .unwrap_or_else(|| duration_from_hms(0, 0, 0));

        let number_of_intervals =
            divide_round_up(duration.num_minutes(), interval_duration.num_minutes());
        let number_of_intervals: usize = number_of_intervals.try_into().unwrap_or_default();
        // println!(
        //     "Number of intervals: {}/{}={}",
        //     duration.num_minutes(),
        //     interval_duration.num_minutes(),
        //     number_of_intervals
        // );
        let cheapest_price =
            get_price_for_cheapest_period(prices, number_of_intervals, &start_day, &end_day)
                .unwrap_or(10.0);

        let is_cheap = current_price.per_kwh <= cheapest_price;
        info!("Cheapest price: {cheapest_price:?} {is_cheap}",);

        if is_cheap {
            ds.last_cheap_update = Some(*now);
        } else {
            ds.last_cheap_update = None;
        }

        self.day = ds;

        let category = prices_to_category(prices, self.category);

        let ps = PriceSummary {
            category,
            is_cheap_2hr: is_cheap,
            per_kwh: current_price.per_kwh,
            next_update: current_price.end_time,
        };
        info!("Price summary: {:?} --> {ps:?}", self.category);
        ps
    }
}

fn new_day_state(now: &DateTime<Utc>) -> DayState {
    let (start_day, end_day) = get_2hr_day(now);
    DayState {
        start: start_day,
        end: end_day,
        cheap_power_for_day: duration_from_hms(0, 0, 0),
        last_cheap_update: None,
    }
}

fn get_2hr_day(now: &DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let time_2hr_cheap: Time = Time::from_hms_opt(5, 0, 0).unwrap_or_default();
    let (start_day, end_day) = get_day(now, time_2hr_cheap, &Local);
    (start_day, end_day)
}

fn get_day<T: TimeZone + std::fmt::Debug>(
    now: &DateTime<Utc>,
    time: Time,
    local: &T,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let today = now.with_timezone(local).date_naive();
    let tomorrow = today + Duration::days(1);
    let mut start_day = convert_date_time_to_utc_or_default(today, time, local);
    let mut end_day = convert_date_time_to_utc_or_default(tomorrow, time, local);
    if *now < start_day {
        start_day -= Duration::days(1);
        end_day -= Duration::days(1);
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]
    use chrono::Local;
    use float_cmp::assert_approx_eq;

    use super::*;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

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
            let end_time = start_time + Duration::minutes(30);
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
    fn test_price_processor() {
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
            let end_time = start_time + Duration::minutes(30);
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
        let mut pp = PriceProcessor::new(&now);

        let prices = vec![
            pr(dt("2020-01-01T00:30:00Z"), 0.0, CurrentInterval),
            pr(dt("2020-01-01T01:00:00Z"), 0.0, ForecastInterval),
            pr(dt("2020-01-01T01:30:00Z"), 10.0, ForecastInterval),
            pr(dt("2020-01-01T02:00:00Z"), 0.0, ForecastInterval),
            pr(dt("2020-01-01T02:30:00Z"), 0.0, ForecastInterval),
            pr(dt("2020-01-01T03:30:00Z"), -10.0, ForecastInterval),
            pr(dt("2020-01-01T04:00:00Z"), -10.0, ForecastInterval),
            pr(dt("2020-01-01T04:30:00Z"), 0.0, ForecastInterval),
            pr(dt("2020-01-01T05:00:00Z"), 10.0, ForecastInterval),
            pr(dt("2020-01-01T05:30:00Z"), -10.0, ForecastInterval),
        ];

        let summary = pp.prices_to_summary(&now, &prices);
        assert_eq!(summary.category, PriceCategory::SuperCheap);
        assert_eq!(summary.is_cheap_2hr, true);
        assert_approx_eq!(f32, summary.per_kwh, 0.0);
        assert_eq!(summary.next_update, dt("2020-01-01T01:00:00Z"));
        let ds = &pp.day;
        assert_eq!(ds.cheap_power_for_day, Duration::minutes(0));
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

        let now: DateTime<Utc> = dt("2020-01-01T01:15:00Z");
        let summary = pp.prices_to_summary(&now, &prices);
        assert_eq!(summary.category, PriceCategory::SuperCheap);
        assert_eq!(summary.is_cheap_2hr, false);
        assert_approx_eq!(f32, summary.per_kwh, 0.0);
        assert_eq!(summary.next_update, dt("2020-01-01T01:30:00Z"));
        let ds = pp.day;
        assert_eq!(ds.cheap_power_for_day, Duration::minutes(45));
        let cp = ds.last_cheap_update;
        assert_eq!(cp, None);
    }

    #[test]
    fn test_get_price_category() {
        use PriceCategory::{Cheap, Expensive, Normal, SuperCheap};

        let data = [
            (SuperCheap, 11.0, SuperCheap),
            (SuperCheap, 11.1, Cheap),
            (SuperCheap, 16.0, Cheap),
            (SuperCheap, 16.1, Normal),
            (SuperCheap, 31.0, Normal),
            (SuperCheap, 31.1, Expensive),
            (Cheap, 8.9, SuperCheap),
            (Cheap, 9.0, Cheap),
            (Cheap, 16.0, Cheap),
            (Cheap, 16.1, Normal),
            (Cheap, 31.0, Normal),
            (Cheap, 31.1, Expensive),
            (Normal, 8.9, SuperCheap),
            (Normal, 9.0, Cheap),
            (Normal, 13.9, Cheap),
            (Normal, 14.0, Normal),
            (Normal, 31.0, Normal),
            (Normal, 31.1, Expensive),
            (Expensive, 8.9, SuperCheap),
            (Expensive, 9.0, Cheap),
            (Expensive, 13.9, Cheap),
            (Expensive, 14.0, Normal),
            (Expensive, 28.9, Normal),
            (Expensive, 29.0, Expensive),
        ];

        for d in data {
            let c = get_price_category(Some(d.0), d.1);
            assert_eq!(c, d.2, "get_price_category({:?}, {}) = {:?}", d.0, d.1, c);
        }
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
            let time = Time::from_hms_opt(5, 0, 0).unwrap();
            let now = dt("2020-01-02T00:00:00Z");
            let (start, stop) = get_day(&now, time, &timezone);
            assert_eq!(start, dt("2020-01-01T18:00:00Z"));
            assert_eq!(stop, dt("2020-01-02T18:00:00Z"));
        }

        {
            let time = Time::from_hms_opt(5, 0, 0).unwrap();
            let now = dt("2020-01-02T17:59:59Z");
            let (start, stop) = get_day(&now, time, &timezone);
            assert_eq!(start, dt("2020-01-01T18:00:00Z"));
            assert_eq!(stop, dt("2020-01-02T18:00:00Z"));
        }

        {
            let time = Time::from_hms_opt(5, 0, 0).unwrap();
            let now = "2020-01-02T18:00:00Z".parse().unwrap();
            let (start, stop) = get_day(&now, time, &timezone);
            assert_eq!(start, dt("2020-01-02T18:00:00Z"));
            assert_eq!(stop, dt("2020-01-03T18:00:00Z"));
        }

        {
            let time = Time::from_hms_opt(5, 0, 0).unwrap();
            let now = "2020-01-02T18:00:01Z".parse().unwrap();
            let (start, stop) = get_day(&now, time, &timezone);
            assert_eq!(start, dt("2020-01-02T18:00:00Z"));
            assert_eq!(stop, dt("2020-01-03T18:00:00Z"));
        }
    }
}
