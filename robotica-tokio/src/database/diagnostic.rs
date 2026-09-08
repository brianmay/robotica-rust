//! Access `diagnostic_events` table in the database
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// Insert a diagnostic event into the database.
///
/// # Arguments
///
/// * `postgres` - The `PostgreSQL` connection pool.
/// * `device_id` - The device identifier (e.g. "`tesla_1`", "`hot_water`")
/// * `event_type` - Type of event (e.g. "`plan_selection`", "`price_update`")
/// * `payload` - JSON payload with event-specific data
///
/// # Errors
///
/// This function can return an error if there is a problem with the database connection or query execution.
pub async fn insert_event(
    postgres: &PgPool,
    device_id: &str,
    event_type: &str,
    payload: serde_json::Value,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        INSERT INTO diagnostic_events (device_id, event_type, payload)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
        device_id,
        event_type,
        payload
    )
    .fetch_one(postgres)
    .await?;

    Ok(i64::from(result.id))
}

/// Simplified price info for diagnostics (avoids serializing Duration)
#[derive(Debug, Serialize, Deserialize)]
pub struct PriceInfo {
    /// Start time of the interval (RFC3339)
    pub start_time: String,
    /// End time of the interval (RFC3339)
    pub end_time: String,
    /// Price per kWh in cents
    pub per_kwh: f32,
    /// Spot price per kWh in cents
    pub spot_per_kwh: f32,
    /// Renewables percentage
    pub renewables: f32,
    /// Interval type
    pub interval_type: String,
    /// Spike status
    pub spike_status: String,
}

/// Payload for plan selection diagnostic events
#[derive(Debug, Serialize, Deserialize)]
pub struct PlanSelectionPayload {
    /// Battery level at time of decision
    pub battery_level: u8,
    /// Whether the device was on (charging/heating)
    pub is_on: bool,
    /// Whether the car was actively charging
    pub is_charging: bool,
    /// Timestamp of the decision
    pub now: String,
    /// The previous plan (if any)
    pub old_plan: Option<PlanInfo>,
    /// The new plan that was chosen
    pub new_plan: Option<PlanInfo>,
    /// Human-readable description of the decision
    pub decision: String,
    /// Reason for the decision
    pub decision_reason: String,
    /// Simplified price list for reference
    pub prices: Vec<PriceInfo>,
}

/// Information about a selected plan
#[derive(Debug, Serialize, Deserialize)]
pub struct PlanInfo {
    /// Plan start time (RFC3339)
    pub start_time: String,
    /// Plan end time (RFC3339)
    pub end_time: String,
    /// Duration in seconds
    pub duration_secs: i64,
    /// Power in kW
    pub kw: f32,
    /// Charge request (e.g. "ChargeTo(70)")
    pub request: String,
    /// Total cost of the plan
    pub cost: f32,
    /// Cost per hour
    pub cost_per_hour: f32,
}
