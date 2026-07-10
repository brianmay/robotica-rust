use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

#[allow(unused_imports)]
pub use robotica_common::robotica::amber::price::{
    AdvancedPrice, ChannelType, Descriptor, IntervalType, PeriodType, PriceResponse, SeasonType,
    TariffInformation,
};

#[derive(Deserialize)]
pub struct Config {
    pub token: String,
    pub site_id: String,
}

#[allow(clippy::enum_variant_names)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    Estimated,
    Billable,
}

#[allow(clippy::enum_variant_names)]
#[derive(Deserialize, Debug)]
pub enum UsageType {
    Usage,
}

/// Amber usage response
#[allow(dead_code)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsageResponse {
    #[serde(rename = "type")]
    pub usage_type: UsageType,
    pub duration: u16,
    pub spot_per_kwh: f32,
    pub per_kwh: f32,
    pub date: NaiveDate,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub nem_time: DateTime<Utc>,
    pub renewables: f32,
    pub channel_type: ChannelType,
    pub tariff_information: TariffInformation,
    pub spike_status: String,
    pub descriptor: Descriptor,
    pub channel_identifier: String,
    pub kwh: f32,
    pub quality: Quality,
    pub cost: f32,
}

pub async fn get_prices(
    config: &Config,
    start_date: NaiveDate,
    end_date: NaiveDate,
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

    response.error_for_status()?.json().await
}

pub async fn get_usage(
    config: &Config,
    start_date: NaiveDate,
    end_date: NaiveDate,
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

    response.error_for_status()?.json().await
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::bool_assert_comparison)]

    use super::*;

    fn dt(dt: impl Into<String>) -> DateTime<Utc> {
        dt.into().parse().unwrap()
    }

    #[test]
    fn test_is_current() {
        let pr = |start_time: DateTime<Utc>,
                  end_time: DateTime<Utc>,
                  interval_type: IntervalType| PriceResponse {
            date: start_time.with_timezone(&Utc).date_naive(),
            start_time,
            end_time,
            nem_time: start_time,
            per_kwh: 0.0,
            spot_per_kwh: 0.0,
            interval_type,
            renewables: 0.0,
            duration: 0,
            channel_type: ChannelType::General,
            descriptor: Descriptor::Neutral,
            estimate: Some(false),
            spike_status: "None".to_string(),
            advanced_price: None,
            tariff_information: TariffInformation {
                period: PeriodType::Peak,
                season: None,
                block: None,
                demand_window: None,
            },
        };

        let now = dt("2020-01-01T00:00:00Z");
        let p = pr(
            dt("2020-01-01T00:00:00Z"),
            dt("2020-01-01T00:30:00Z"),
            IntervalType::CurrentInterval,
        );
        assert_eq!(p.is_current(now), true);

        let p = pr(
            dt("2020-01-01T00:00:00Z"),
            dt("2020-01-01T00:00:00Z"),
            IntervalType::ActualInterval,
        );
        assert_eq!(p.is_current(now), false);

        let p = pr(
            dt("2020-01-01T00:00:00Z"),
            dt("2020-01-01T00:00:01Z"),
            IntervalType::CurrentInterval,
        );
        assert_eq!(p.is_current(now), true);

        let p = pr(
            dt("2019-01-01T23:59:59Z"),
            dt("2020-01-01T00:00:00Z"),
            IntervalType::ActualInterval,
        );
        assert_eq!(p.is_current(now), false);

        let p = pr(
            dt("2019-01-01T23:59:59Z"),
            dt("2020-01-01T00:00:01Z"),
            IntervalType::CurrentInterval,
        );
        assert_eq!(p.is_current(now), true);
    }
}
