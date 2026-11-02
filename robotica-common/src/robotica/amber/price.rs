//! Amber price types, shared between the backend and the frontend.
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tap::Pipe;
use tracing::{error, info};

use crate::robotica::entities::Id;

/// The type of an Amber price interval.
#[allow(clippy::enum_variant_names)]
#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, PartialEq)]
pub enum IntervalType {
    /// The interval has passed and has actual price data.
    ActualInterval,

    /// The interval is in the future and the price is a forecast.
    ForecastInterval,

    /// The interval is the current interval.
    CurrentInterval,
}

/// The Amber channel a price applies to.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ChannelType {
    /// General usage channel.
    General,

    /// Controlled load channel.
    ControlledLoad,

    /// Solar feed in channel.
    FeedIn,
}

/// The tariff period of a price interval.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PeriodType {
    /// Off peak period.
    OffPeak,

    /// Shoulder period.
    Shoulder,

    /// Solar sponge period.
    SolarSponge,

    /// Peak period.
    Peak,
}

/// The tariff season of a price interval.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SeasonType {
    /// Default season.
    Default,

    /// Summer season.
    Summer,

    /// Autumn season.
    Autumn,

    /// Winter season.
    Winter,

    /// Spring season.
    Spring,

    /// Non summer season.
    NonSummer,

    /// Holiday season.
    Holiday,

    /// Weekend season.
    Weekend,

    /// Weekend holiday season.
    WeekendHoliday,

    /// Weekday season.
    Weekday,
}

/// Tariff information for a price interval.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TariffInformation {
    /// The tariff period.
    pub period: PeriodType,

    /// The tariff season.
    pub season: Option<SeasonType>,

    /// The tariff block.
    pub block: Option<u32>,

    /// Is this a demand window?
    pub demand_window: Option<bool>,
}

/// Amber advanced (forecast) price, present on current and forecast intervals.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedPrice {
    /// The low forecast price, in cents per kWh.
    pub low: f32,

    /// The predicted price, in cents per kWh.
    pub predicted: f32,

    /// The high forecast price, in cents per kWh.
    pub high: f32,
}

/// Amber price descriptor, a human-friendly bucket for the interval price.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Descriptor {
    /// The price is negative.
    Negative,

    /// The price is extremely low.
    ExtremelyLow,

    /// The price is very low.
    VeryLow,

    /// The price is low.
    Low,

    /// The price is neutral.
    Neutral,

    /// The price is high.
    High,

    /// The price is a spike.
    Spike,
}

/// Amber price response for a single interval.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceResponse {
    /// The type of the interval.
    #[serde(rename = "type")]
    pub interval_type: IntervalType,

    /// The duration of the interval in minutes.
    pub duration: u16,

    /// The spot price, in cents per kWh.
    pub spot_per_kwh: f32,

    /// The price, in cents per kWh.
    pub per_kwh: f32,

    /// The date of the interval.
    pub date: NaiveDate,

    /// The start time of the interval.
    pub start_time: DateTime<Utc>,

    /// The end time of the interval.
    pub end_time: DateTime<Utc>,

    /// The NEM time of the interval.
    pub nem_time: DateTime<Utc>,

    /// The percentage of renewables in the grid.
    pub renewables: f32,

    /// The channel the price applies to.
    pub channel_type: ChannelType,

    /// The tariff information for the interval.
    pub tariff_information: Option<TariffInformation>,

    /// The spike status of the interval.
    pub spike_status: String,

    /// The price descriptor of the interval.
    pub descriptor: Descriptor,

    /// Is the price an estimate?
    pub estimate: Option<bool>,

    /// The advanced (forecast) price, if available.
    pub advanced_price: Option<AdvancedPrice>,
}

impl PriceResponse {
    /// Is this interval current at the given time?
    #[must_use]
    pub fn is_current(&self, dt: DateTime<Utc>) -> bool {
        self.start_time <= dt && self.end_time > dt
    }

    /// Get the price per kWh to use for calculations.
    ///
    /// Prefers Amber's predicted advanced price when available, falling back to
    /// the reported `per_kwh` price otherwise.
    #[must_use]
    pub const fn effective_per_kwh(&self) -> f32 {
        match &self.advanced_price {
            Some(advanced_price) => advanced_price.predicted,
            None => self.per_kwh,
        }
    }
}

/// The default pricing interval, used when the interval cannot be derived
/// from the price data.
const DEFAULT_INTERVAL: Duration = crate::datetime::duration::minutes(5);

/// Amber prices for all channels.
///
/// This is the form published to MQTT: the price responses partitioned by
/// channel, together with the pricing interval and the time the prices
/// were last updated.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Prices {
    /// Prices for the general usage channel.
    pub general: Vec<PriceResponse>,

    /// Prices for the controlled load channel.
    pub controlled_load: Vec<PriceResponse>,

    /// Prices for the solar feed in channel.
    pub feed_in: Vec<PriceResponse>,

    /// The duration of each pricing interval.
    #[serde(with = "crate::datetime::with_duration")]
    pub interval: Duration,

    /// When the prices were last updated.
    pub last_updated: DateTime<Utc>,
}

impl Prices {
    fn get_vec_for_type(prices: &[PriceResponse], channel_type: ChannelType) -> Vec<PriceResponse> {
        let count = prices
            .iter()
            .filter(|pr| pr.channel_type == channel_type)
            .count();
        Vec::with_capacity(count)
    }

    /// Create a `Prices` from raw Amber price responses.
    ///
    /// The responses are partitioned by channel. The interval is derived
    /// from the last response, falling back to the default interval when
    /// the price data is empty or invalid. `last_updated` records when
    /// the prices were fetched.
    #[must_use]
    pub fn new(id: &Id, last_updated: DateTime<Utc>, prices: Vec<PriceResponse>) -> Self {
        let interval = prices.last().map_or_else(
            || {
                error!(%id, "No interval found in prices: {prices:?}");
                // If we failed to get an interval, just use default
                DEFAULT_INTERVAL
            },
            |last_price| {
                // If this produces an error, end time must have been before start time!
                (last_price.end_time - last_price.start_time)
                    .to_std()
                    .unwrap_or(DEFAULT_INTERVAL)
            },
        );

        let mut general = Self::get_vec_for_type(&prices, ChannelType::General);
        let mut controlled_load = Self::get_vec_for_type(&prices, ChannelType::ControlledLoad);
        let mut feed_in = Self::get_vec_for_type(&prices, ChannelType::FeedIn);

        for pr in prices {
            match pr.channel_type {
                ChannelType::General => general.push(pr),
                ChannelType::ControlledLoad => controlled_load.push(pr),
                ChannelType::FeedIn => feed_in.push(pr),
            }
        }

        Self {
            general,
            controlled_load,
            feed_in,
            interval,
            last_updated,
        }
    }

    /// Get a view over the general channel prices.
    #[must_use]
    pub fn general(&self) -> GeneralPrices<'_> {
        GeneralPrices {
            list: &self.general,
            interval: self.interval,
        }
    }

    /// Get the current price of each channel at `dt`.
    ///
    /// Yields at most one price per channel, in combined channel order.
    #[must_use]
    pub fn current(&self, dt: &DateTime<Utc>) -> Vec<&PriceResponse> {
        let mut result = Vec::with_capacity(3);
        if let Some(pr) = self.general.iter().find(|pr| pr.is_current(*dt)) {
            result.push(pr);
        }
        if let Some(pr) = self.controlled_load.iter().find(|pr| pr.is_current(*dt)) {
            result.push(pr);
        }
        if let Some(pr) = self.feed_in.iter().find(|pr| pr.is_current(*dt)) {
            result.push(pr);
        }
        result
    }

    /// Iterate over the prices of all channels, grouped by channel.
    pub fn combined_iter(&self) -> impl Iterator<Item = &PriceResponse> {
        self.general
            .iter()
            .chain(self.controlled_load.iter())
            .chain(self.feed_in.iter())
    }
}

/// A view over the general channel prices of a [`Prices`].
#[derive(Debug)]
pub struct GeneralPrices<'a> {
    /// The general channel prices.
    pub list: &'a [PriceResponse],

    /// The duration of each pricing interval.
    pub interval: Duration,
}

impl GeneralPrices<'_> {
    /// Get the general channel price current at `dt`.
    #[must_use]
    pub fn current(&self, dt: &DateTime<Utc>) -> Option<&PriceResponse> {
        self.list.iter().find(|pr| pr.is_current(*dt))
    }

    /// Get the start time of the next pricing period after `now`.
    #[must_use]
    pub fn get_next_period(&self, id: &Id, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let Ok(interval) = i64::try_from(self.interval.as_secs()) else {
            error!(
                %id,
                "Failed to convert interval to i64: {}",
                self.interval.as_secs()
            );
            return None;
        };

        let timestamp = now.timestamp();
        let value = timestamp + interval - timestamp % interval;
        DateTime::from_timestamp(value, 0)
    }

    /// Get the general channel price at `dt`, weighted with the
    /// neighbouring intervals.
    #[must_use]
    pub fn get_weighted_price(&self, id: &Id, dt: DateTime<Utc>) -> Option<f32> {
        let prices = &self.list;
        let pos = prices.iter().position(|pr| pr.is_current(dt))?;

        let prefix_pos = if pos > 0 { pos - 1 } else { 0 };
        let postfix_pos = if pos + 1 < prices.len() { pos + 1 } else { pos };

        let prefix = prices[prefix_pos].effective_per_kwh();
        let current = prices[pos].effective_per_kwh();
        let postfix = prices[postfix_pos].effective_per_kwh();

        let values = [prefix, current, postfix];
        let weights = [25u8, 50u8, 25u8];
        let total_weights = f32::from(weights.iter().map(|x| u16::from(*x)).sum::<u16>());

        let result = values
            .iter()
            .zip(weights.iter())
            .map(|(v, w)| v * f32::from(*w))
            .sum::<f32>()
            .pipe(|x| x / total_weights);

        info!(%id, "Get Weighted Price: {values:?} {weights:?} --> {result}");
        Some(result)
    }

    /// Find the general channel price current at `now`.
    #[must_use]
    pub fn find(&self, now: DateTime<Utc>) -> Option<&PriceResponse> {
        self.list.iter().find(|pr| pr.is_current(now))
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

    fn pr_channel(
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        per_kwh: f32,
        channel_type: ChannelType,
    ) -> PriceResponse {
        PriceResponse {
            date: start_time.with_timezone(&Local).date_naive(),
            start_time,
            end_time,
            per_kwh,
            spot_per_kwh: per_kwh,
            interval_type: IntervalType::ActualInterval,
            renewables: 0.0,
            duration: 30,
            channel_type,
            nem_time: start_time,
            descriptor: Descriptor::Neutral,
            estimate: Some(false),
            spike_status: "None".to_string(),
            advanced_price: None,
            tariff_information: Some(TariffInformation {
                period: PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            }),
        }
    }

    const INTERVAL: Duration = crate::datetime::duration::minutes(30);

    #[test]
    fn test_price_response_round_trip() {
        let json = serde_json::json!({
            "type": "CurrentInterval",
            "duration": 30,
            "spotPerKwh": 6.25,
            "perKwh": 24.5,
            "date": "2026-07-08",
            "startTime": "2026-07-08T02:00:01Z",
            "endTime": "2026-07-08T02:30:00Z",
            "nemTime": "2026-07-08T12:30:00Z",
            "renewables": 45.0,
            "channelType": "general",
            "tariffInformation": {
                "period": "offPeak",
                "season": "winter",
                "block": null,
                "demandWindow": false,
            },
            "spikeStatus": "none",
            "descriptor": "low",
            "estimate": false,
            "advancedPrice": {
                "low": 20.0,
                "predicted": 25.0,
                "high": 30.0,
            },
        });

        let price: PriceResponse = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(price.interval_type, IntervalType::CurrentInterval);
        assert_eq!(price.channel_type, ChannelType::General);
        assert_eq!(price.descriptor, Descriptor::Low);
        assert!((price.effective_per_kwh() - 25.0).abs() < f32::EPSILON);

        let serialized = serde_json::to_value(&price).unwrap();
        assert_eq!(serialized, json);
    }

    #[rstest::rstest]
    #[case(dt("2020-01-01T00:00:00Z"), dt("2020-01-01T00:30:00Z"))]
    #[case(dt("2020-01-01T00:00:01Z"), dt("2020-01-01T00:30:00Z"))]
    #[case(dt("2020-01-01T00:00:00.55Z"), dt("2020-01-01T00:30:00Z"))]
    #[case(dt("2020-01-01T00:10:00Z"), dt("2020-01-01T00:30:00Z"))]
    #[case(dt("2020-01-01T00:29:00Z"), dt("2020-01-01T00:30:00Z"))]
    #[case(dt("2020-01-01T00:30:00Z"), dt("2020-01-01T01:00:00Z"))]
    fn test_get_next_period(#[case] now: DateTime<Utc>, #[case] expected: DateTime<Utc>) {
        let prices = GeneralPrices {
            list: &[],
            interval: INTERVAL,
        };
        let id = Id::new("test").unwrap();

        let result = prices.get_next_period(&id, now).unwrap();
        assert_eq!(result, expected);
    }

    #[rstest::rstest]
    #[case(dt("2020-01-01T00:00:00Z"), 1.0)]
    #[case(dt("2020-01-01T00:10:00Z"), 1.0)]
    #[case(dt("2020-01-01T00:29:00Z"), 1.0)]
    #[case(dt("2020-01-01T00:30:00Z"), 2.0)]
    fn test_find(#[case] now: DateTime<Utc>, #[case] expected: f32) {
        let pr = |start_time: DateTime<Utc>,
                  end_time: DateTime<Utc>,
                  price,
                  interval_type: IntervalType| {
            PriceResponse {
                date: start_time.with_timezone(&Local).date_naive(),
                start_time,
                end_time,
                per_kwh: price,
                spot_per_kwh: price,
                interval_type,
                renewables: 0.0,
                duration: 0,
                channel_type: ChannelType::General,
                nem_time: start_time,
                descriptor: Descriptor::Neutral,
                estimate: Some(false),
                spike_status: "None".to_string(),
                advanced_price: None,
                tariff_information: Some(TariffInformation {
                    period: PeriodType::Peak,
                    season: None,
                    block: None,
                    demand_window: None,
                }),
            }
        };

        let prices = vec![
            pr(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                1.0,
                IntervalType::ActualInterval,
            ),
            pr(
                dt("2020-01-01T00:30:00Z"),
                dt("2020-01-01T01:00:00Z"),
                2.0,
                IntervalType::ActualInterval,
            ),
            pr(
                dt("2020-01-01T01:00:00Z"),
                dt("2020-01-01T01:30:00Z"),
                4.0,
                IntervalType::ActualInterval,
            ),
        ];
        let prices = GeneralPrices {
            list: &prices,
            interval: INTERVAL,
        };

        let result = prices.find(now).unwrap().per_kwh;
        assert_approx_eq!(f32, result, expected);
    }

    #[rstest::rstest]
    #[case(dt("2020-01-01T00:00:00Z"))]
    #[case(dt("2020-01-01T00:10:00Z"))]
    #[case(dt("2020-01-01T00:29:00Z"))]
    fn test_find_none(#[case] now: DateTime<Utc>) {
        let prices = GeneralPrices {
            list: &[],
            interval: INTERVAL,
        };

        let result = prices.find(now);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_weighted_price() {
        let id = Id::new("test").unwrap();

        let pr = |start_time: DateTime<Utc>,
                  end_time: DateTime<Utc>,
                  price,
                  interval_type: IntervalType| {
            PriceResponse {
                date: start_time.with_timezone(&Local).date_naive(),
                start_time,
                end_time,
                per_kwh: price,
                spot_per_kwh: price,
                interval_type,
                renewables: 0.0,
                duration: 0,
                channel_type: ChannelType::General,
                nem_time: start_time,
                descriptor: Descriptor::Neutral,
                estimate: Some(false),
                spike_status: "None".to_string(),
                advanced_price: None,
                tariff_information: Some(TariffInformation {
                    period: PeriodType::Peak,
                    season: None,
                    block: None,
                    demand_window: None,
                }),
            }
        };

        let it = |current, n: i32| match n.cmp(&current) {
            std::cmp::Ordering::Less => IntervalType::ActualInterval,
            std::cmp::Ordering::Equal => IntervalType::CurrentInterval,
            std::cmp::Ordering::Greater => IntervalType::ForecastInterval,
        };

        let prices_fn = |current| {
            vec![
                pr(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    1.0,
                    it(current, 0),
                ),
                pr(
                    dt("2020-01-01T00:30:00Z"),
                    dt("2020-01-01T01:00:00Z"),
                    2.0,
                    it(current, 1),
                ),
                pr(
                    dt("2020-01-01T01:00:00Z"),
                    dt("2020-01-01T01:30:00Z"),
                    4.0,
                    it(current, 2),
                ),
            ]
        };

        let now = dt("2020-01-01T00:00:00Z");
        let prices = prices_fn(0);
        let prices = GeneralPrices {
            list: &prices,
            interval: INTERVAL,
        };
        let p = prices.get_weighted_price(&id, now).unwrap();
        assert_approx_eq!(f32, p, 1.25);

        let now = dt("2020-01-01T00:30:00Z");
        let prices = prices_fn(1);
        let prices = GeneralPrices {
            list: &prices,
            interval: INTERVAL,
        };
        let p = prices.get_weighted_price(&id, now).unwrap();
        assert_approx_eq!(f32, p, 2.25);

        let now = dt("2020-01-01T01:00:00Z");
        let prices = prices_fn(2);
        let prices = GeneralPrices {
            list: &prices,
            interval: INTERVAL,
        };
        let p = prices.get_weighted_price(&id, now).unwrap();
        assert_approx_eq!(f32, p, 3.5);

        let now = dt("2020-01-01T01:30:00Z");
        let prices = prices_fn(3);
        let prices = GeneralPrices {
            list: &prices,
            interval: INTERVAL,
        };
        let p = prices.get_weighted_price(&id, now);
        assert!(p.is_none());
    }

    #[test]
    fn test_prices_new_partitions_by_channel_type() {
        let id = Id::new("test").unwrap();

        // Input interleaves channel types to prove they are partitioned.
        let prices = vec![
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                1.0,
                ChannelType::General,
            ),
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                2.0,
                ChannelType::ControlledLoad,
            ),
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                3.0,
                ChannelType::FeedIn,
            ),
            pr_channel(
                dt("2020-01-01T00:30:00Z"),
                dt("2020-01-01T01:00:00Z"),
                4.0,
                ChannelType::General,
            ),
            pr_channel(
                dt("2020-01-01T00:30:00Z"),
                dt("2020-01-01T01:00:00Z"),
                5.0,
                ChannelType::ControlledLoad,
            ),
            pr_channel(
                dt("2020-01-01T00:30:00Z"),
                dt("2020-01-01T01:00:00Z"),
                6.0,
                ChannelType::FeedIn,
            ),
        ];
        let prices = Prices::new(&id, dt("2020-01-01T00:00:00Z"), prices);

        // The interval is derived from the last price's start/end times.
        assert_eq!(prices.interval, INTERVAL);

        // The general view exposes only the general prices, in input order.
        let general = prices.general();
        assert_eq!(general.list.len(), 2);
        assert!(general
            .list
            .iter()
            .all(|pr| pr.channel_type == ChannelType::General));
        assert_approx_eq!(f32, general.list[0].per_kwh, 1.0);
        assert_approx_eq!(f32, general.list[1].per_kwh, 4.0);

        // The combined iterator yields every price, grouped by channel type.
        let channel_types: Vec<_> = prices.combined_iter().map(|pr| pr.channel_type).collect();
        assert_eq!(
            channel_types,
            vec![
                ChannelType::General,
                ChannelType::General,
                ChannelType::ControlledLoad,
                ChannelType::ControlledLoad,
                ChannelType::FeedIn,
                ChannelType::FeedIn,
            ]
        );
    }

    #[test]
    fn test_prices_new_interval_fallbacks_to_default() {
        let id = Id::new("test").unwrap();

        // An empty price list has no interval to derive one from.
        let prices = Prices::new(&id, dt("2020-01-01T00:00:00Z"), Vec::new());
        assert_eq!(prices.interval, DEFAULT_INTERVAL);

        // A last price with end_time before start_time yields a negative
        // interval, which must also fall back to the default.
        let last = pr_channel(
            dt("2020-01-01T00:30:00Z"),
            dt("2020-01-01T00:00:00Z"),
            1.0,
            ChannelType::General,
        );
        let prices = Prices::new(&id, dt("2020-01-01T00:00:00Z"), vec![last]);
        assert_eq!(prices.interval, DEFAULT_INTERVAL);
    }

    #[test]
    fn test_prices_general() {
        let id = Id::new("test").unwrap();
        let prices = Prices::new(
            &id,
            dt("2020-01-01T00:00:00Z"),
            vec![
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    1.0,
                    ChannelType::General,
                ),
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    2.0,
                    ChannelType::ControlledLoad,
                ),
                pr_channel(
                    dt("2020-01-01T00:30:00Z"),
                    dt("2020-01-01T01:00:00Z"),
                    4.0,
                    ChannelType::General,
                ),
            ],
        );

        let general = prices.general();

        // The view shares the interval and exposes only general prices.
        assert_eq!(general.interval, prices.interval);
        assert_eq!(general.list.len(), 2);

        // The view supports looking up the current general price.
        let current = general.current(&dt("2020-01-01T00:15:00Z")).unwrap();
        assert_eq!(current.channel_type, ChannelType::General);
        assert_approx_eq!(f32, current.per_kwh, 1.0);
    }

    #[test]
    fn test_prices_current() {
        let id = Id::new("test").unwrap();

        let prices = Prices::new(
            &id,
            dt("2020-01-01T00:00:00Z"),
            vec![
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    1.0,
                    ChannelType::General,
                ),
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    2.0,
                    ChannelType::ControlledLoad,
                ),
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    3.0,
                    ChannelType::FeedIn,
                ),
                pr_channel(
                    dt("2020-01-01T00:30:00Z"),
                    dt("2020-01-01T01:00:00Z"),
                    4.0,
                    ChannelType::General,
                ),
            ],
        );

        // At most one current price per channel type, in combined order.
        let current = prices.current(&dt("2020-01-01T00:15:00Z"));
        assert_eq!(current.len(), 3);
        assert_eq!(current[0].channel_type, ChannelType::General);
        assert_eq!(current[1].channel_type, ChannelType::ControlledLoad);
        assert_eq!(current[2].channel_type, ChannelType::FeedIn);

        // Only the general channel has a current interval.
        let current = prices.current(&dt("2020-01-01T00:45:00Z"));
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].channel_type, ChannelType::General);
        assert_approx_eq!(f32, current[0].per_kwh, 4.0);

        // Overlapping current intervals in one channel yield only the first.
        let overlapping = Prices::new(
            &id,
            dt("2020-01-01T00:00:00Z"),
            vec![
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:40:00Z"),
                    1.0,
                    ChannelType::General,
                ),
                pr_channel(
                    dt("2020-01-01T00:10:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    9.0,
                    ChannelType::General,
                ),
            ],
        );
        let current = overlapping.current(&dt("2020-01-01T00:15:00Z"));
        assert_eq!(current.len(), 1);
        assert_approx_eq!(f32, current[0].per_kwh, 1.0);

        // Nothing is current outside all intervals.
        assert_eq!(
            prices.current(&dt("2020-01-01T02:00:00Z")),
            [] as [&PriceResponse; 0]
        );
    }

    #[test]
    fn test_prices_equality() {
        let id = Id::new("test").unwrap();

        let prices_fn = || {
            vec![
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    1.0,
                    ChannelType::General,
                ),
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    2.0,
                    ChannelType::ControlledLoad,
                ),
                pr_channel(
                    dt("2020-01-01T00:00:00Z"),
                    dt("2020-01-01T00:30:00Z"),
                    3.0,
                    ChannelType::FeedIn,
                ),
            ]
        };

        // Equal content in separately allocated instances is equal, even
        // when the input interleaves channel types differently.
        let a = Prices::new(&id, dt("2020-01-01T00:00:00Z"), prices_fn());
        let mut reversed = prices_fn();
        reversed.reverse();
        let b = Prices::new(&id, dt("2020-01-01T00:00:00Z"), reversed);
        assert_eq!(a, b);

        // Different content is not equal.
        let mut other = prices_fn();
        other[0].per_kwh = 100.0;
        let c = Prices::new(&id, dt("2020-01-01T00:00:00Z"), other);
        assert_ne!(a, c);

        // A different last_updated is not equal.
        let d = Prices::new(&id, dt("2020-01-01T00:00:01Z"), prices_fn());
        assert_ne!(a, d);
    }

    #[test]
    fn test_prices_serialize_round_trip() {
        let id = Id::new("test").unwrap();
        let last_updated = dt("2020-01-01T00:00:00Z");

        // Input interleaves channel types.
        let prices = vec![
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                1.0,
                ChannelType::General,
            ),
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                2.0,
                ChannelType::ControlledLoad,
            ),
            pr_channel(
                dt("2020-01-01T00:00:00Z"),
                dt("2020-01-01T00:30:00Z"),
                3.0,
                ChannelType::FeedIn,
            ),
            pr_channel(
                dt("2020-01-01T00:30:00Z"),
                dt("2020-01-01T01:00:00Z"),
                4.0,
                ChannelType::General,
            ),
        ];
        let prices = Prices::new(&id, last_updated, prices);

        // The MQTT payload is an object with a field per channel, not the
        // flat list the frontend previously depended on.
        let json = serde_json::to_value(&prices).unwrap();
        let object = json.as_object().unwrap();
        assert_eq!(object.len(), 5);
        assert!(object.contains_key("general"));
        assert!(object.contains_key("controlled_load"));
        assert!(object.contains_key("feed_in"));
        assert_eq!(object["interval"], "00:30:00");
        assert_eq!(object["last_updated"], "2020-01-01T00:00:00Z");

        // The payload deserializes back into an equal Prices.
        let deserialized: Prices = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, prices);
    }
}
