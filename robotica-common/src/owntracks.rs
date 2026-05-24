//! Structures for `OwnTracks` MQTT location messages.
//!
//! See <https://owntracks.org/booklet/tech/json/> for the full specification.

use serde::Deserialize;

/// Battery status reported by `OwnTracks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "u8")]
pub enum BatteryStatus {
    /// Battery status is unknown.
    Unknown,
    /// Device is unplugged.
    Unplugged,
    /// Device is charging.
    Charging,
    /// Battery is full.
    Full,
}

impl TryFrom<u8> for BatteryStatus {
    type Error = String;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Unplugged),
            2 => Ok(Self::Charging),
            3 => Ok(Self::Full),
            other => Err(format!("unknown battery status value: {other}")),
        }
    }
}

/// Internet connectivity status reported by `OwnTracks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ConnectivityStatus {
    /// Connected via `WiFi`.
    #[serde(rename = "w")]
    Wifi,
    /// Offline.
    #[serde(rename = "o")]
    Offline,
    /// Mobile data.
    #[serde(rename = "m")]
    Mobile,
}

/// Location report trigger reported by `OwnTracks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Trigger {
    /// Ping issued randomly by background task.
    #[serde(rename = "p")]
    Ping,
    /// Circular region enter/leave event.
    #[serde(rename = "c")]
    CircularRegion,
    /// Circular region enter/leave event for +follow regions (iOS).
    #[serde(rename = "C")]
    CircularRegionFollow,
    /// Beacon region enter/leave event (iOS).
    #[serde(rename = "b")]
    Beacon,
    /// Response to a `reportLocation` cmd message.
    #[serde(rename = "r")]
    ReportLocation,
    /// Manual publish requested by the user.
    #[serde(rename = "u")]
    UserRequest,
    /// Timer based publish in move mode (iOS).
    #[serde(rename = "t")]
    Timer,
    /// Updated by iOS Frequent Locations monitoring.
    #[serde(rename = "v")]
    FrequentLocations,
}

/// An `OwnTracks` `_type=location` MQTT message.
///
/// All optional fields that the device may omit are wrapped in `Option`.
/// The two required fields (`lat`, `lon`, `tst`) are non-optional.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LocationMessage {
    /// Latitude in degrees.
    pub lat: f64,

    /// Longitude in degrees.
    pub lon: f64,

    /// UNIX epoch timestamp (seconds) of the GPS fix.
    pub tst: i64,

    /// Accuracy of the reported location in metres.
    #[serde(default)]
    pub acc: Option<i32>,

    /// Altitude above sea level in metres.
    #[serde(default)]
    pub alt: Option<i32>,

    /// Device battery level as a percentage.
    #[serde(default)]
    pub batt: Option<u8>,

    /// Battery status.
    #[serde(default)]
    pub bs: Option<BatteryStatus>,

    /// Course over ground in degrees (iOS).
    #[serde(default)]
    pub cog: Option<i32>,

    /// Radius around the region when entering/leaving in metres (iOS).
    #[serde(default)]
    pub rad: Option<i32>,

    /// Trigger for the location report.
    #[serde(default, rename = "t")]
    pub trigger: Option<Trigger>,

    /// Tracker ID (last two characters of the topic by default).
    #[serde(default)]
    pub tid: Option<String>,

    /// Vertical accuracy of the `alt` element in metres (iOS).
    #[serde(default)]
    pub vac: Option<i32>,

    /// Velocity in km/h.
    #[serde(default)]
    pub vel: Option<i32>,

    /// Barometric pressure in kPa (iOS, extended data).
    #[serde(default, rename = "p")]
    pub pressure: Option<f32>,

    /// Point of interest name (iOS).
    #[serde(default)]
    pub poi: Option<String>,

    /// Internet connectivity status (extended data).
    #[serde(default)]
    pub conn: Option<ConnectivityStatus>,

    /// Name tag (iOS).
    #[serde(default)]
    pub tag: Option<String>,

    /// Original publish topic (HTTP payloads only).
    #[serde(default)]
    pub topic: Option<String>,

    /// List of region names the device is currently inside.
    #[serde(default)]
    pub inregions: Vec<String>,

    /// List of region IDs the device is currently inside.
    #[serde(default)]
    pub inrids: Vec<String>,

    /// SSID of the connected `WiFi` network (iOS).
    #[serde(default, rename = "SSID")]
    pub ssid: Option<String>,

    /// BSSID of the connected access point (iOS).
    #[serde(default, rename = "BSSID")]
    pub bssid: Option<String>,

    /// Time at which the message was constructed (if different from `tst`).
    #[serde(default)]
    pub created_at: Option<i64>,

    /// Monitoring mode: 1 = significant, 2 = move (iOS).
    #[serde(default, rename = "m")]
    pub monitoring_mode: Option<u8>,

    /// Random identifier for correlating send/return messages (Android).
    #[serde(default, rename = "_id")]
    pub id: Option<String>,

    /// Motion states detected by iOS motion manager.
    #[serde(default)]
    pub motionactivities: Vec<String>,
}
