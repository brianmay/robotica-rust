//! Get information from Amber electricity supplier

use chrono::{FixedOffset, Local, TimeZone, Utc};
use influxdb::InfluxDbWriteable;
use log::debug;
use serde::Deserialize;
use thiserror::Error;
use tokio::time::{interval, sleep_until, Instant, MissedTickBehavior};

use robotica_backend::{
    entities::{self, Receiver, StatefulData},
    get_env, is_debug_mode, spawn, EnvironmentError,
};
use robotica_common::datetime::{
    convert_date_time_to_utc, utc_now, Date, DateTime, Duration, Time,
};

/// Error when starting the Amber service
#[derive(Error, Debug)]
pub enum AmberError {
    /// Environment variable not found
    #[error("Environment variable error: {0}")]
    EnvironmentError(#[from] EnvironmentError),
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
    per_kwh: u32,
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
fn hours(num: u16) -> u16 {
    num * HOURS_TO_SECONDS
}

/// Get the current electricity price from Amber
///
/// # Errors
///
/// Returns an `AmberError` if the required environment variables are not set.
///
pub fn run() -> Result<Receiver<StatefulData<PriceSummary>>, AmberError> {
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

    let (tx, rx) = entities::create_stateful_entity("amber_summary");

    spawn(async move {
        // if is_debug_mode() {
        //     let start_date = Date::from_ymd(2022, 1, 1);
        //     let stop_date = Date::from_ymd(2022, 3, 1);
        //     // process_prices(&config, start_date, stop_date).await;
        //     process_usage(&config, start_date, stop_date).await;
        //     println!("------------------- done -------------------");
        // }
        let mut pp = PriceProcessor::new();
        let nem_timezone = FixedOffset::east(hours(10).into());

        // Update prices maximum every 5 minutes
        let mut price_instant = Instant::now() + tokio::time::Duration::from_secs(0);

        // Update usage once an hour
        let mut usage_interval = interval(tokio::time::Duration::from_secs(hours(1).into()));
        usage_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = sleep_until(price_instant) => {
                    let now = utc_now();
                    let today = now.with_timezone(&nem_timezone).date();
                    let yesterday = today - Duration::days(1);
                    let tomorrow = today + Duration::days(1);

                    // Get prices for the current interval.
                    let prices = get_prices(&config, yesterday, tomorrow).await;

                    // Process the results.
                    let next_delay = match prices {
                        Ok(prices) => {
                            // Update the summary.
                            let summary = pp.prices_to_summary(&now, &prices);
                            let update_time = summary.next_update.clone();

                            // Write the prices to influxdb and send
                            prices_to_influxdb(&config, &prices, &summary).await;
                            tx.try_send(summary);

                            // Add margin to allow time for Amber to update.
                            let update_time = update_time + Duration::seconds(5);

                            // How long to the current interval expires?
                            let now = utc_now();
                            let duration = update_time.clone() - now;
                            log::info!("Next price update: {update_time:?} in {duration}");

                            // Ensure we update prices at least once once every 5 minutes.
                            let max_duration = Duration::minutes(5);
                            let min_duration = Duration::seconds(30);
                            duration.clamp(min_duration, max_duration)
                        }
                        Err(err) => {
                            log::error!("Failed to get prices: {}", err);
                            // If we failed to get prices, try again in 1 minute
                            Duration::minutes(1)
                        }
                    };

                    // Schedule the next update
                    log::info!("Next poll in {}", next_delay);
                    let next_delay: std::time::Duration = next_delay.to_std().unwrap_or(std::time::Duration::from_secs(300));
                    price_instant = Instant::now() + next_delay;
                }
                _ = usage_interval.tick() => {
                    // Update the amber usage once an hour.
                    let now = utc_now();
                    let today = now.with_timezone(&nem_timezone).date();
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
        influxdb::Type::Text(v.to_string())
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PriceCategory {
    SuperCheap,
    Cheap,
    Normal,
    Expensive,
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PriceSummary {
    pub category: PriceCategory,
    pub is_cheap_2hr: bool,
    pub per_kwh: u32,
    pub next_update: DateTime<Utc>,
}

fn prices_to_category(prices: &[PriceResponse]) -> PriceCategory {
    let mut category: PriceCategory = PriceCategory::Normal;

    prices
        .iter()
        .filter(|p| p.interval_type == IntervalType::CurrentInterval)
        .map(|price| {
            if price.per_kwh < 10.0 {
                PriceCategory::SuperCheap
            } else if price.per_kwh < 15.0 {
                PriceCategory::Cheap
            } else if price.per_kwh < 30.0 {
                PriceCategory::Normal
            } else {
                PriceCategory::Expensive
            }
        })
        .for_each(|new_pq| category = new_pq);

    category
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
            time: data.start_time.clone().into(),
            interval_type: data.interval_type,
        }
        .into_query("amber/price");

        if let Err(e) = client.query(&reading).await {
            log::error!("Failed to write to influxdb: {}", e);
        }
    }

    let reading = PriceSummaryReading {
        is_cheap_2hr: summary.is_cheap_2hr,
        per_kwh: summary.per_kwh,
        time: Utc::now(),
    }
    .into_query("amber/price_summary");

    if let Err(e) = client.query(&reading).await {
        log::error!("Failed to write to influxdb: {}", e);
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
                    time: data.start_time.into(),
                }
                .into_query(name);

                if let Err(e) = client.query(&reading).await {
                    log::error!("Failed to write to influxdb: {}", e);
                }
            }
        }
        Err(e) => {
            log::error!("Failed to get usage: {}", e);
        }
    }
}

#[derive(Debug, Clone)]
struct DayState {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    cheap_power_for_day: Duration,
    last_cheap_update: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct PriceProcessor {
    day: Option<DayState>,
}

impl PriceProcessor {
    pub fn new() -> Self {
        Self { day: None }
    }

    pub fn prices_to_summary(
        &mut self,
        now: &DateTime<Utc>,
        prices: &[PriceResponse],
    ) -> PriceSummary {
        let current_price = prices
            .iter()
            .find(|p| p.interval_type == IntervalType::CurrentInterval)
            .unwrap();

        let time = Time::new(5, 0, 0);
        let (start_day, end_day) = get_day(now, time, &Local);

        let new_day = || DayState {
            start: start_day.clone(),
            end: end_day.clone(),
            cheap_power_for_day: Duration::new(0, 0, 0),
            last_cheap_update: None,
        };

        let mut ds = if let Some(ds) = &self.day {
            if *now < ds.start || *now >= ds.end {
                new_day()
            } else {
                ds.clone()
            }
        } else {
            new_day()
        };

        if let Some(last_cheap_update) = &ds.last_cheap_update {
            let duration = now.clone() - last_cheap_update.clone();
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
            .unwrap_or_else(|| Duration::new(0, 0, 0));

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
            get_price_for_cheapest_period(prices, number_of_intervals, start_day, end_day)
                .unwrap_or(10.0);

        let is_cheap = current_price.per_kwh <= cheapest_price;
        log::info!("Cheapest price: {cheapest_price:?} {is_cheap}",);

        if is_cheap {
            ds.last_cheap_update = Some(now.clone());
        } else {
            ds.last_cheap_update = None;
        }

        self.day = Some(ds);

        let ps = PriceSummary {
            category: prices_to_category(prices),
            is_cheap_2hr: is_cheap,
            per_kwh: current_price.per_kwh.round() as u32,
            next_update: current_price.end_time.clone(),
        };
        log::info!("Price summary: {:?}", ps);
        ps
    }
}

fn get_day<T: TimeZone + std::fmt::Debug>(
    now: &DateTime<Utc>,
    time: Time,
    local: &T,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let today = now.with_timezone(local).date();
    let tomorrow = today + Duration::days(1);
    // FIXME: Don't use unwrap here.
    let mut start_day = convert_date_time_to_utc(today, time, local).unwrap();
    let mut end_day = convert_date_time_to_utc(tomorrow, time, local).unwrap();
    if *now < start_day {
        start_day = start_day - Duration::days(1);
        end_day = end_day - Duration::days(1);
    }
    (start_day, end_day)
}

/// Divide two numbers and round up
fn divide_round_up(dividend: i64, divisor: i64) -> i64 {
    (dividend + divisor - 1) / divisor
}

fn get_price_for_cheapest_period(
    prices: &[PriceResponse],
    number_of_intervals: usize,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Option<f32> {
    if number_of_intervals == 0 {
        return None;
    }

    let mut prices: Vec<_> = prices
        .iter()
        .filter(|p| {
            p.start_time >= start_time
                && p.start_time < end_time
                && p.interval_type != IntervalType::ActualInterval
        })
        .map(|p| p.per_kwh)
        .collect();

    prices.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // println!("Prices: {prices:?} {number_of_intervals}");

    prices
        .get(number_of_intervals - 1)
        .or_else(|| prices.last())
        .cloned()
}

#[cfg(test)]
mod tests {
    use chrono::Local;

    use super::*;

    #[test]
    fn test_get_price_for_cheapest_period() {
        let tariff_information = TariffInformation {
            period: PeriodType::Peak,
            season: None,
            block: None,
            demand_window: None,
        };

        let pr = |start_time: DateTime<Utc>, price| {
            let date = start_time.with_timezone(&Local).date();
            let end_time = start_time.clone() + Duration::minutes(30);
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
            pr("2020-01-01T00:30:00Z".parse().unwrap(), -10.0),
            pr("2020-01-01T01:00:00Z".parse().unwrap(), 0.0),
            pr("2020-01-01T01:30:00Z".parse().unwrap(), 10.0),
            pr("2020-01-01T02:00:00Z".parse().unwrap(), 0.0),
            pr("2020-01-01T02:30:00Z".parse().unwrap(), 0.0),
            pr("2020-01-01T03:30:00Z".parse().unwrap(), -10.0),
            pr("2020-01-01T04:00:00Z".parse().unwrap(), 0.0),
            pr("2020-01-01T04:30:00Z".parse().unwrap(), 0.0),
            pr("2020-01-01T05:00:00Z".parse().unwrap(), 10.0),
            pr("2020-01-01T05:30:00Z".parse().unwrap(), -10.0),
            pr("2020-01-01T06:00:00Z".parse().unwrap(), -10.0),
        ];

        let start_time: DateTime<Utc> = "2020-01-01T00:00:00Z".parse().unwrap();
        let end_time: DateTime<Utc> = "2020-01-01T06:30:00Z".parse().unwrap();
        assert_eq!(
            get_price_for_cheapest_period(&prices, 0, start_time.clone(), end_time.clone()),
            None
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 1, start_time.clone(), end_time.clone()),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 2, start_time.clone(), end_time.clone()),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 3, start_time.clone(), end_time.clone()),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 4, start_time.clone(), end_time.clone()),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 5, start_time, end_time),
            Some(0.0)
        );

        let start_time: DateTime<Utc> = "2020-01-01T00:00:00Z".parse().unwrap();
        let end_time: DateTime<Utc> = "2020-01-01T06:00:00Z".parse().unwrap();
        assert_eq!(
            get_price_for_cheapest_period(&prices, 0, start_time.clone(), end_time.clone()),
            None
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 1, start_time.clone(), end_time.clone()),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 2, start_time.clone(), end_time.clone()),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 3, start_time.clone(), end_time.clone()),
            Some(-10.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 4, start_time.clone(), end_time.clone()),
            Some(0.0)
        );
        assert_eq!(
            get_price_for_cheapest_period(&prices, 5, start_time, end_time),
            Some(-0.0)
        );
    }

    #[test]
    fn test_price_processor() {
        let tariff_information = TariffInformation {
            period: PeriodType::Peak,
            season: None,
            block: None,
            demand_window: None,
        };

        let pr = |start_time: DateTime<Utc>, price, interval_type| {
            let date = start_time.with_timezone(&Local).date();
            let end_time = start_time.clone() + Duration::minutes(30);
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

        use IntervalType::ActualInterval;
        use IntervalType::CurrentInterval;
        use IntervalType::ForecastInterval;

        let mut pp = PriceProcessor::new();

        let prices = vec![
            pr(
                "2020-01-01T00:30:00Z".parse().unwrap(),
                0.0,
                CurrentInterval,
            ),
            pr(
                "2020-01-01T01:00:00Z".parse().unwrap(),
                0.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T01:30:00Z".parse().unwrap(),
                10.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T02:00:00Z".parse().unwrap(),
                0.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T02:30:00Z".parse().unwrap(),
                0.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T03:30:00Z".parse().unwrap(),
                -10.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T04:00:00Z".parse().unwrap(),
                -10.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T04:30:00Z".parse().unwrap(),
                0.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T05:00:00Z".parse().unwrap(),
                10.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T05:30:00Z".parse().unwrap(),
                -10.0,
                ForecastInterval,
            ),
        ];

        let now = "2020-01-01T00:30:00Z".parse().unwrap();
        assert_eq!(
            pp.prices_to_summary(&now, &prices),
            PriceSummary {
                category: PriceCategory::SuperCheap,
                is_cheap_2hr: true,
                per_kwh: 0,
                next_update: "2020-01-01T01:00:00Z".parse().unwrap(),
            }
        );
        let ds = pp.day.clone().unwrap();
        assert_eq!(ds.cheap_power_for_day, Duration::minutes(0));
        let cp = ds.last_cheap_update.unwrap();
        assert_eq!(cp, now);

        let prices = vec![
            pr("2020-01-01T00:30:00Z".parse().unwrap(), 0.0, ActualInterval),
            pr(
                "2020-01-01T01:00:00Z".parse().unwrap(),
                0.0,
                CurrentInterval,
            ),
            pr(
                "2020-01-01T01:30:00Z".parse().unwrap(),
                0.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T02:00:00Z".parse().unwrap(),
                20.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T02:30:00Z".parse().unwrap(),
                20.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T03:30:00Z".parse().unwrap(),
                -30.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T04:00:00Z".parse().unwrap(),
                -30.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T04:30:00Z".parse().unwrap(),
                -30.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T05:00:00Z".parse().unwrap(),
                30.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T05:30:00Z".parse().unwrap(),
                40.0,
                ForecastInterval,
            ),
            pr(
                "2020-01-01T06:00:00Z".parse().unwrap(),
                40.0,
                ForecastInterval,
            ),
        ];

        let now: DateTime<Utc> = "2020-01-01T01:15:00Z".parse().unwrap();
        assert_eq!(
            pp.prices_to_summary(&now, &prices),
            PriceSummary {
                category: PriceCategory::SuperCheap,
                is_cheap_2hr: false,
                per_kwh: 0,
                next_update: "2020-01-01T01:30:00Z".parse().unwrap(),
            }
        );
        let ds = pp.day.unwrap();
        assert_eq!(ds.cheap_power_for_day, Duration::minutes(45));
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
        let timezone = FixedOffset::east(60 * 60 * 11);
        {
            let time = Time::new(5, 0, 0);
            let now = "2020-01-02T00:00:00Z".parse().unwrap();
            let (start, stop) = get_day(&now, time, &timezone);
            assert_eq!(start, "2020-01-01T18:00:00Z".parse().unwrap());
            assert_eq!(stop, "2020-01-02T18:00:00Z".parse().unwrap());
        }

        {
            let time = Time::new(5, 0, 0);
            let now = "2020-01-02T17:59:59Z".parse().unwrap();
            let (start, stop) = get_day(&now, time, &timezone);
            assert_eq!(start, "2020-01-01T18:00:00Z".parse().unwrap());
            assert_eq!(stop, "2020-01-02T18:00:00Z".parse().unwrap());
        }

        {
            let time = Time::new(5, 0, 0);
            let now = "2020-01-02T18:00:00Z".parse().unwrap();
            let (start, stop) = get_day(&now, time, &timezone);
            assert_eq!(start, "2020-01-02T18:00:00Z".parse().unwrap());
            assert_eq!(stop, "2020-01-03T18:00:00Z".parse().unwrap());
        }

        {
            let time = Time::new(5, 0, 0);
            let now = "2020-01-02T18:00:01Z".parse().unwrap();
            let (start, stop) = get_day(&now, time, &timezone);
            assert_eq!(start, "2020-01-02T18:00:00Z".parse().unwrap());
            assert_eq!(stop, "2020-01-03T18:00:00Z".parse().unwrap());
        }
    }
}
