//! Amber price types, shared between the backend and the frontend.
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

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
    pub tariff_information: TariffInformation,

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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

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
}
