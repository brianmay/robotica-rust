//! Get information from Amber electricity supplier

use chrono::NaiveTime;
use chrono::{FixedOffset, Local, TimeZone, Utc};
use influxdb::InfluxDbWriteable;
use serde::{Deserialize, Serialize};
use tap::Pipe;
use thiserror::Error;
use tokio::time::{interval, sleep_until, Instant, MissedTickBehavior};
use tracing::{error, info};

use robotica_backend::{pipes::stateful, services::persistent_state::PersistentStateRow, spawn};
use robotica_common::datetime::{
    convert_date_time_to_utc_or_default, duration_from_hms, utc_now, Date, DateTime, Duration, Time,
};

use crate::influxdb as influx;
use crate::InitState;

/// Error when starting the Amber service
#[derive(Error, Debug)]
pub enum Error {
    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

#[derive(Deserialize)]
pub struct Config {
    pub token: String,
    pub site_id: String,
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

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ChargeRequest {
    ChargeTo(u8),
    //DoNotCharge,
}

/// Outputs from Amber
pub struct OutputsReceiver {
    // pub price_category: stateful::Receiver<PriceCategory>,
    pub is_cheap_2hr: stateful::Receiver<bool>,
    pub charge_request: stateful::Receiver<ChargeRequest>,
}

pub struct OutputsSender {
    // pub price_category: stateful::Sender<PriceCategory>,
    pub is_cheap_2hr: stateful::Sender<bool>,
    pub charge_request: stateful::Sender<ChargeRequest>,
}

fn new_outputs() -> (OutputsSender, OutputsReceiver) {
    // let (price_category_tx, price_category_rx) = stateful::create_pipe("amber_output_category");
    let (is_cheap_2hr_tx, is_cheap_2hr_rx) = stateful::create_pipe("amber_output_is_cheap_2hr");
    let (charge_request_tx, charge_request_rx) =
        stateful::create_pipe("amber_output_charge_request");

    (
        OutputsSender {
            // price_category: price_category_tx,
            is_cheap_2hr: is_cheap_2hr_tx,
            charge_request: charge_request_tx,
        },
        OutputsReceiver {
            // price_category: price_category_rx,
            is_cheap_2hr: is_cheap_2hr_rx,
            charge_request: charge_request_rx,
        },
    )
}

/// Get the current electricity price from Amber
///
/// # Errors
///
/// Returns an `AmberError` if the required environment variables are not set.
///
pub fn run(
    state: &InitState,
    config: Config,
    influxdb_config: &influx::Config,
) -> Result<OutputsReceiver, Error> {
    let (tx, rx) = new_outputs();

    let psr = state
        .persistent_state_database
        .for_name::<DayState>("amber");

    let nem_timezone = FixedOffset::east_opt(hours(10).into())
        .ok_or_else(|| Error::Internal("Failed to create NEM timezone".to_string()))?;

    let influxdb_config = influxdb_config.clone();

    spawn(async move {
        // if hack enabled {
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
                () = sleep_until(price_instant) => {
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

                            if let Some(summary) = summary {
                                let update_time = summary.next_update;

                                let charge_request = summary_to_charge_request(&summary, now, &Local);

                                // Write the prices to influxdb and send
                                prices_to_influxdb(&influxdb_config, &prices, &summary).await;
                                // tx.price_category.try_send(summary.category);
                                tx.is_cheap_2hr.try_send(summary.is_cheap_2hr);
                                tx.charge_request.try_send(charge_request);

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
                            } else {
                                // If we failed to get a summary, try again in 1 minute
                                info!("Retry in 1 minute");
                                Duration::minutes(1)
                            }
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
                    process_usage(&config, &influxdb_config, yesterday, tomorrow).await;
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
struct PriceSummary {
    category: PriceCategory,
    is_cheap_2hr: bool,
    c_per_kwh: f32,
    next_update: DateTime<Utc>,
}

fn get_price_category(category: Option<PriceCategory>, price: f32) -> PriceCategory {
    let mut c = category.unwrap_or(PriceCategory::Normal);

    let under = |c: PriceCategory, threshold: f32, new_category: PriceCategory| {
        // If all prices are under the threshold, then change the category.
        if price < threshold {
            new_category
        } else {
            c
        }
    };
    let over = |c: PriceCategory, threshold: f32, new_category: PriceCategory| {
        // If the current price is over the threshold, then change the category.
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

async fn prices_to_influxdb(
    influxdb_config: &influx::Config,
    prices: &[PriceResponse],
    summary: &PriceSummary,
) {
    let client = influxdb_config.get_client();

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
        per_kwh: summary.c_per_kwh,
        time: Utc::now(),
    }
    .into_query("amber/price_summary");

    if let Err(e) = client.query(&reading).await {
        error!("Failed to write to influxdb: {}", e);
    }
}

async fn process_usage(
    config: &Config,
    influxdb_config: &influx::Config,
    start_date: Date,
    end_date: Date,
) {
    let usage = get_usage(config, start_date, end_date).await;
    match usage {
        Ok(usage) => {
            let client = influxdb_config.get_client();

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

    #[allow(clippy::cognitive_complexity)]
    pub fn prices_to_summary(
        &mut self,
        now: &DateTime<Utc>,
        prices: &[PriceResponse],
    ) -> Option<PriceSummary> {
        let Some(current_price) = get_current_price_response(prices, now) else {
            error!("No current price found in prices: {prices:?}");
            return None;
        };

        let Some(weighted_price) = get_weighted_price(prices, now) else {
            error!("No current price found in prices: {prices:?}");
            return None;
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

        // let category = prices_to_category(prices, self.category);
        let old_category = self.category;
        // let prices_per_kwh: Vec<f32> = current_prices.iter().map(|p| p.per_kwh).collect();
        let category = get_price_category(old_category, weighted_price);
        self.category = Some(category);

        #[allow(clippy::cast_possible_truncation)]
        let ps = PriceSummary {
            category,
            is_cheap_2hr: is_cheap,
            c_per_kwh: current_price.per_kwh,
            next_update: current_price.end_time,
        };
        info!("Price summary: {old_category:?} --> {ps:?}");

        Some(ps)
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

fn is_period_current(pr: &PriceResponse, dt: &DateTime<Utc>) -> bool {
    pr.start_time <= *dt && pr.end_time > *dt
}

fn get_current_price_response<'a>(
    prices: &'a [PriceResponse],
    dt: &DateTime<Utc>,
) -> Option<&'a PriceResponse> {
    prices.iter().find(|pr| is_period_current(pr, dt))
}

fn get_weighted_price(prices: &[PriceResponse], dt: &DateTime<Utc>) -> Option<f32> {
    let pos = prices.iter().position(|pr| is_period_current(pr, dt));

    let Some(pos) = pos else {
        return None;
    };

    let prefix_pos = if pos > 0 { pos - 1 } else { 0 };
    let postfix_pos = if pos + 1 < prices.len() { pos + 1 } else { pos };

    let prefix = prices[prefix_pos].per_kwh;
    let current = prices[pos].per_kwh;
    let postfix = prices[postfix_pos].per_kwh;

    let values = [prefix, current, postfix];
    let weights = [25u8, 50u8, 25u8];
    let total_weights = f32::from(weights.iter().map(|x| u16::from(*x)).sum::<u16>());

    let result = values
        .iter()
        .zip(weights.iter())
        .map(|(v, w)| v * f32::from(*w))
        .sum::<f32>()
        .pipe(|x| x / total_weights);

    info!("Prices {values:?} {weights:?} --> {result}");

    Some(result)
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

fn summary_to_charge_request<T: TimeZone>(
    summary: &PriceSummary,
    dt: DateTime<Utc>,
    tz: &T,
) -> ChargeRequest {
    let now = dt.with_timezone(tz);
    let time = now.time();

    #[allow(clippy::unwrap_used)]
    let start_time = NaiveTime::from_hms_opt(3, 0, 0).unwrap();
    #[allow(clippy::unwrap_used)]
    let end_time = NaiveTime::from_hms_opt(6, 30, 0).unwrap();
    let force = time > start_time && time < end_time;

    #[allow(clippy::match_same_arms)]
    match (force, summary.category) {
        (_, PriceCategory::SuperCheap) => ChargeRequest::ChargeTo(90),
        (_, PriceCategory::Cheap) => ChargeRequest::ChargeTo(80),
        (true, PriceCategory::Normal) => ChargeRequest::ChargeTo(70),
        (false, PriceCategory::Normal) => ChargeRequest::ChargeTo(50),
        (true, PriceCategory::Expensive) => ChargeRequest::ChargeTo(50),
        (false, PriceCategory::Expensive) => ChargeRequest::ChargeTo(20),
    }
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

        let summary = pp.prices_to_summary(&now, &prices).unwrap();
        assert_eq!(summary.category, PriceCategory::SuperCheap);
        assert_eq!(summary.is_cheap_2hr, true);
        assert_approx_eq!(f32, summary.c_per_kwh, 0.0);
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
        let summary = pp.prices_to_summary(&now, &prices).unwrap();
        assert_eq!(summary.category, PriceCategory::SuperCheap);
        assert_eq!(summary.is_cheap_2hr, false);
        assert_approx_eq!(f32, summary.c_per_kwh, 0.0);
        assert_eq!(summary.next_update, dt("2020-01-01T01:30:00Z"));
        let ds = pp.day;
        assert_eq!(ds.cheap_power_for_day, Duration::minutes(45));
        let cp = ds.last_cheap_update;
        assert_eq!(cp, None);
    }

    #[test]
    fn test_get_price_category() {
        use PriceCategory::{Cheap, Expensive, Normal, SuperCheap};

        // For < thresholds, all prices must be < threshold
        // For > thresholds, only current price must be > threshold
        let data = [
            // Super cheap thresholds >11.0 Cheap >16.0 Normal >31.0 Expensive
            (SuperCheap, 10.0, SuperCheap),
            (SuperCheap, 11.1, Cheap),
            (SuperCheap, 16.0, Cheap),
            (SuperCheap, 16.1, Normal),
            (SuperCheap, 31.0, Normal),
            (SuperCheap, 31.1, Expensive),
            // Cheap thresholds >16.0 Normal >31.0 Expensive <9.0 SuperCheap
            (Cheap, 8.9, SuperCheap),
            (Cheap, 9.0, Cheap),
            (Cheap, 11.1, Cheap),
            (Cheap, 16.0, Cheap),
            (Cheap, 16.1, Normal),
            (Cheap, 31.0, Normal),
            (Cheap, 31.1, Expensive),
            // Normal thresholds >31.0 Expensive <14.0 Cheap <9.0 SuperCheap
            (Normal, 8.9, SuperCheap),
            (Normal, 9.0, Cheap),
            (Normal, 13.9, Cheap),
            (Normal, 14.0, Normal),
            (Normal, 31.0, Normal),
            (Normal, 31.1, Expensive),
            // Expensive thresholds <29.0 Normal <14.0 Cheap <9.0 SuperCheap
            (Expensive, 8.9, SuperCheap),
            (Expensive, 9.0, Cheap),
            (Expensive, 13.9, Cheap),
            (Expensive, 14.0, Normal),
            (Expensive, 28.9, Normal),
            (Expensive, 29.0, Expensive),
        ];

        for d in data {
            let c = get_price_category(Some(d.0), d.1);
            assert_eq!(c, d.2, "get_price_category({:?}, {:?}) = {:?}", d.0, d.1, c);
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

    #[test]
    fn test_is_period_current() {
        let pr = |start_time: DateTime<Utc>, end_time: DateTime<Utc>| PriceResponse {
            date: start_time.with_timezone(&Local).date_naive(),
            start_time,
            end_time,
            per_kwh: 0.0,
            spot_per_kwh: 0.0,
            interval_type: IntervalType::CurrentInterval,
            renewables: 0.0,
            duration: 0,
            channel_type: ChannelType::General,
            estimate: Some(false),
            spike_status: "None".to_string(),
            tariff_information: TariffInformation {
                period: PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            },
        };

        let now = dt("2020-01-01T00:00:00Z");
        let p = pr(dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z"));
        assert_eq!(is_period_current(&p, &now), true);

        let p = pr(dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:00:00Z"));
        assert_eq!(is_period_current(&p, &now), false);

        let p = pr(dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:00:01Z"));
        assert_eq!(is_period_current(&p, &now), true);

        let p = pr(dt("2019-01-01T23:59:59Z"), dt("2020-01-01T00:00:00Z"));
        assert_eq!(is_period_current(&p, &now), false);

        let p = pr(dt("2019-01-01T23:59:59Z"), dt("2020-01-01T00:00:01Z"));
        assert_eq!(is_period_current(&p, &now), true);
    }

    #[test]
    fn test_get_current_price_response() {
        let pr = |start_time: DateTime<Utc>, end_time: DateTime<Utc>| PriceResponse {
            date: start_time.with_timezone(&Local).date_naive(),
            start_time,
            end_time,
            per_kwh: 0.0,
            spot_per_kwh: 0.0,
            interval_type: IntervalType::CurrentInterval,
            renewables: 0.0,
            duration: 0,
            channel_type: ChannelType::General,
            estimate: Some(false),
            spike_status: "None".to_string(),
            tariff_information: TariffInformation {
                period: PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            },
        };

        let prices = vec![
            pr(dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z")),
            pr(dt("2020-01-01T00:30:00Z"), dt("2020-01-01T01:00:00Z")),
            pr(dt("2020-01-01T01:00:00Z"), dt("2020-01-01T01:30:00Z")),
        ];

        let now = dt("2019-12-31T23:59:59Z");
        let p = get_current_price_response(&prices, &now);
        assert!(p.is_none());

        let now = dt("2020-01-01T00:00:00Z");
        let p = get_current_price_response(&prices, &now).unwrap();
        assert_eq!(p.start_time, prices[0].start_time);
        assert_eq!(p.end_time, prices[0].end_time);

        let now = dt("2020-01-01T00:30:00Z");
        let p = get_current_price_response(&prices, &now).unwrap();
        assert_eq!(p.start_time, prices[1].start_time);
        assert_eq!(p.end_time, prices[1].end_time);

        let now = dt("2020-01-01T01:00:00Z");
        let p = get_current_price_response(&prices, &now).unwrap();
        assert_eq!(p.start_time, prices[2].start_time);
        assert_eq!(p.end_time, prices[2].end_time);

        let now = dt("2020-01-01T01:30:00Z");
        let p = get_current_price_response(&prices, &now);
        assert!(p.is_none());
    }

    #[test]
    fn test_get_weighted_price() {
        let pr = |start_time: DateTime<Utc>, end_time: DateTime<Utc>, price| PriceResponse {
            date: start_time.with_timezone(&Local).date_naive(),
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
            tariff_information: TariffInformation {
                period: PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            },
        };

        let prices = vec![
            pr(dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z"), 1.0),
            pr(dt("2020-01-01T00:30:00Z"), dt("2020-01-01T01:00:00Z"), 2.0),
            pr(dt("2020-01-01T01:00:00Z"), dt("2020-01-01T01:30:00Z"), 4.0),
        ];

        let now = dt("2020-01-01T00:00:00Z");
        let p = get_weighted_price(&prices, &now).unwrap();
        assert_approx_eq!(f32, p, 1.25);

        let now = dt("2020-01-01T00:30:00Z");
        let p = get_weighted_price(&prices, &now).unwrap();
        assert_approx_eq!(f32, p, 2.25);

        let now = dt("2020-01-01T01:00:00Z");
        let p = get_weighted_price(&prices, &now).unwrap();
        assert_approx_eq!(f32, p, 3.5);

        let now = dt("2020-01-01T01:30:00Z");
        let p = get_weighted_price(&prices, &now);
        assert!(p.is_none());
    }

    #[test]
    fn test_summary_to_charge_request_normal() {
        let now = dt("2020-01-01T00:00:00Z");
        let summary = PriceSummary {
            category: PriceCategory::SuperCheap,
            is_cheap_2hr: true,
            c_per_kwh: 0.0,
            next_update: dt("2020-01-01T01:00:00Z"),
        };
        let cr = summary_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(90));

        let now = dt("2020-01-01T00:00:00Z");
        let summary = PriceSummary {
            category: PriceCategory::Cheap,
            is_cheap_2hr: true,
            c_per_kwh: 0.0,
            next_update: dt("2020-01-01T01:00:00Z"),
        };
        let cr = summary_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(80));

        let now = dt("2020-01-01T00:00:00Z");
        let summary = PriceSummary {
            category: PriceCategory::Normal,
            is_cheap_2hr: true,
            c_per_kwh: 0.0,
            next_update: dt("2020-01-01T01:00:00Z"),
        };
        let cr = summary_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(50));

        let now = dt("2020-01-01T00:00:00Z");
        let summary = PriceSummary {
            category: PriceCategory::Expensive,
            is_cheap_2hr: true,
            c_per_kwh: 0.0,
            next_update: dt("2020-01-01T01:00:00Z"),
        };
        let cr = summary_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(20));
    }

    #[test]
    fn test_summary_to_charge_request_forced() {
        let now = dt("2020-01-01T03:30:00Z");
        let summary = PriceSummary {
            category: PriceCategory::SuperCheap,
            is_cheap_2hr: true,
            c_per_kwh: 0.0,
            next_update: dt("2020-01-01T01:00:00Z"),
        };
        let cr = summary_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(90));

        let now = dt("2020-01-01T03:30:00Z");
        let summary = PriceSummary {
            category: PriceCategory::Cheap,
            is_cheap_2hr: true,
            c_per_kwh: 0.0,
            next_update: dt("2020-01-01T01:00:00Z"),
        };
        let cr = summary_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(80));

        let now = dt("2020-01-01T03:30:00Z");
        let summary = PriceSummary {
            category: PriceCategory::Normal,
            is_cheap_2hr: true,
            c_per_kwh: 0.0,
            next_update: dt("2020-01-01T01:00:00Z"),
        };
        let cr = summary_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(70));

        let now = dt("2020-01-01T03:30:00Z");
        let summary = PriceSummary {
            category: PriceCategory::Expensive,
            is_cheap_2hr: true,
            c_per_kwh: 0.0,
            next_update: dt("2020-01-01T01:00:00Z"),
        };
        let cr = summary_to_charge_request(&summary, now, &Utc);
        assert_eq!(cr, ChargeRequest::ChargeTo(50));
    }
}
