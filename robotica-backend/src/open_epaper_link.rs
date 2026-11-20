//! `OpenEPaperLink` e-paper tag displays.
//!
//! Each display subscribes to its own MQTT topics, renders a template and
//! pushes it to its tag via the `OpenEPaperLink` AP `jsonupload` endpoint.
//! Updates are rate limited so tags running on battery are only refreshed
//! at a bounded rate.

use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use robotica_common::{
    mqtt::Json,
    robotica::{
        entities::{AnyId, Id},
        zones::LocationMessage,
    },
    zwave,
};
use robotica_tokio::{
    pipes::{stateful, stateless, Subscriber, Subscription},
    services::mqtt::Subscriptions,
    spawn,
};
use serde::{Deserialize, Serialize};
use serde_tuple::Serialize_tuple;
use thiserror::Error;
use tracing::{error, info};
use url::Url;

/// Top-level `open_epaper_link` configuration.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    /// The base URL of the `OpenEPaperLink` AP.
    url: Url,

    /// The timezone used to format times displayed on tags.
    #[serde(default)]
    timezone: Tz,

    /// The displays to run.
    displays: Vec<DisplayConfig>,
}

/// Configuration for a single display.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum DisplayConfig {
    /// Show the current locations of a tracked object, subscribing to the
    /// `robotica/state/{id}/locations` topic.
    Locations {
        /// The MAC address of the tag.
        mac: Mac,
        /// The id of the tracked object.
        id: Id,
        /// Minimum interval between display updates, in seconds.
        #[serde(default = "default_rate_limit_secs")]
        rate_limit_secs: u64,
    },

    /// Show a set of sensor readings with the time of the last update.
    Sensors {
        /// The MAC address of the tag.
        mac: Mac,
        /// The title displayed on the tag.
        name: String,
        /// The sensors to display.
        sensors: Vec<SensorConfig>,
        /// Minimum interval between display updates, in seconds.
        #[serde(default = "default_rate_limit_secs")]
        rate_limit_secs: u64,
    },
}

impl DisplayConfig {
    const fn mac(&self) -> &Mac {
        match self {
            Self::Locations { mac, .. } | Self::Sensors { mac, .. } => mac,
        }
    }
}

/// The default minimum interval between display updates.
const fn default_rate_limit_secs() -> u64 {
    300
}

/// Configuration for a single sensor displayed on a tag.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct SensorConfig {
    /// The label displayed for the sensor.
    label: String,

    /// The MQTT topic carrying `zwave::Data<f64>` JSON readings.
    topic: String,

    /// An optional unit suffix (e.g. `"°C"`, `"%"`).
    #[serde(default)]
    unit: Option<String>,
}

/// The MAC address of an `OpenEPaperLink` tag.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Mac(String);

/// A tag to send templates to.
#[derive(Clone, Debug)]
struct Device {
    /// The base URL of the `OpenEPaperLink` AP.
    url: Url,

    /// The MAC address of the tag.
    mac: Mac,
}

#[derive(Serialize_tuple, Debug)]
struct Text {
    x: u16,
    y: u16,
    content: String,
    font: String,
    color: u8,
}

#[derive(Serialize_tuple, Debug)]
struct Box {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    color: u8,
}

#[derive(Serialize_tuple, Debug)]
struct Line {
    x1: u16,
    y1: u16,
    x2: u16,
    y2: u16,
    color: u8,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
enum Element {
    Text(Text),
    #[allow(dead_code)]
    Box(Box),
    Line(Line),
}

#[derive(Serialize, Debug)]
struct Template(Vec<Element>);

#[derive(Error, Debug)]
enum Error {
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("url error: {0}")]
    Url(#[from] url::ParseError),
}

impl Template {
    async fn send(&self, device: &Device) -> Result<(), Error> {
        let serialized = serde_json::to_string(&self)?;
        let url = device.url.join("jsonupload")?;
        info!("Sending template to {url}: {serialized}");

        let mut params = HashMap::new();
        params.insert("mac", device.mac.0.clone());
        params.insert("json", serialized);

        let response = reqwest::Client::new()
            .post(url)
            .form(&params)
            .send()
            .await?
            .error_for_status()?;

        info!("Got response: {response:?}");
        Ok(())
    }
}

const fn pre_inc(y: &mut u16, inc: u16) -> u16 {
    let old_y = *y;
    *y = old_y + inc;
    *y
}

const fn post_inc(y: &mut u16, inc: u16) -> u16 {
    let old_y = *y;
    *y = old_y + inc;
    old_y
}

/// The fonts used by the displays, sized for 152x152 tags.
///
/// The advance widths are measured from the `bahnschrift20.vlw` and
/// `calibrib16.vlw` fonts shipped with the `OpenEPaperLink` AP: the
/// longest sensor lines ("Dew Point: 17.6°C", "Upd: 13:02 20 Nov") fit
/// within 152 px, and the layout fits up to five sensors vertically.
const HEADER_FONT: &str = "fonts/bahnschrift20";
const TEXT_FONT: &str = "fonts/calibrib16";

/// The vertical advance of a display row, in pixels.
const ROW_HEIGHT: u16 = 18;

fn header(y: &mut u16, t: String) -> Element {
    Element::Text(Text {
        x: 5,
        y: post_inc(y, 22),
        content: t,
        font: HEADER_FONT.to_string(),
        color: 2,
    })
}

fn text(y: &mut u16, t: String) -> Element {
    Element::Text(Text {
        x: 5,
        y: pre_inc(y, ROW_HEIGHT),
        content: t,
        font: TEXT_FONT.to_string(),
        color: 1,
    })
}

const fn line(y: &mut u16) -> Element {
    let at = *y;
    *y += 2;
    Element::Line(Line {
        x1: 0,
        y1: at,
        x2: 152,
        y2: at,
        color: 1,
    })
}

/// Start all configured displays.
///
/// Each display subscribes to its own MQTT topics and pushes templates to
/// its tag, rate limited to preserve the tag battery.
pub fn run(config: &Config, subscriptions: &mut Subscriptions) {
    for display in &config.displays {
        let device = Device {
            url: config.url.clone(),
            mac: display.mac().clone(),
        };

        match display {
            DisplayConfig::Locations {
                id,
                rate_limit_secs,
                ..
            } => output_locations(device, id, *rate_limit_secs, subscriptions),
            DisplayConfig::Sensors {
                name,
                sensors,
                rate_limit_secs,
                ..
            } => output_sensors(
                device,
                config.timezone,
                name,
                sensors,
                *rate_limit_secs,
                subscriptions,
            ),
        }
    }
}

/// The label and zone names of a tracked object, as displayed on a tag.
#[derive(Clone, Debug, PartialEq)]
struct LocationsDisplay {
    label: String,
    zones: Vec<String>,
}

/// Show the current locations of a tracked object on a tag.
///
/// Subscribes to the `robotica/state/{id}/locations` topic, which is
/// published by cars and `OwnTracks` sources.
fn output_locations(
    device: Device,
    id: &Id,
    rate_limit_secs: u64,
    subscriptions: &mut Subscriptions,
) {
    let topic = id.get_state_topic("locations");

    let rx = subscriptions
        .subscribe_into_stateful::<Json<LocationMessage>>(&topic)
        .map(|(_, Json(msg))| LocationsDisplay {
            label: msg.label,
            zones: msg.zones.iter().map(|zone| zone.name.clone()).collect(),
        });

    let rx = rx.rate_limit(
        &format!(
            "open_epaper_link/{id}/locations/ratelimit",
            id = id.to_id_string()
        ),
        Duration::from_secs(rate_limit_secs),
    );

    let device = Arc::new(device);
    rx.async_for_each(move |(_, display)| {
        let device = device.clone();
        async move {
            let mut y = 4;
            let mut template = Template(vec![header(&mut y, display.label), line(&mut y)]);
            for zone in display.zones {
                template.0.push(text(&mut y, zone));
            }
            if let Err(e) = template.send(&device).await {
                error!("Failed to send template: {e}");
            }
        }
    });
}

/// A single sensor reading.
#[derive(Clone, Debug, PartialEq)]
struct SensorReading {
    /// The value of the reading.
    value: f64,

    /// The time the reading was taken.
    datetime: Option<DateTime<Utc>>,
}

/// The current readings of a sensor display.
#[derive(Clone, Debug, PartialEq)]
struct SensorsState {
    /// One optional reading per configured sensor, indexed in config order.
    readings: Vec<Option<SensorReading>>,
}

impl SensorsState {
    fn new(count: usize) -> Self {
        Self {
            readings: vec![None; count],
        }
    }

    fn update(&mut self, index: usize, data: &zwave::Data<f64>) {
        if let Some(slot) = self.readings.get_mut(index) {
            *slot = Some(SensorReading {
                value: data.value,
                datetime: data.get_datetime(),
            });
        }
    }

    /// The time of the most recent reading, if any.
    fn last_update(&self) -> Option<DateTime<Utc>> {
        self.readings
            .iter()
            .flatten()
            .filter_map(|reading| reading.datetime)
            .max()
    }
}

/// Show a set of sensor readings on a tag, with the time of the last update.
///
/// Each sensor subscribes to a MQTT topic carrying `zwave::Data<f64>` JSON
/// readings. Updates are rate limited to preserve the tag battery.
fn output_sensors(
    device: Device,
    timezone: Tz,
    name: &str,
    sensors: &[SensorConfig],
    rate_limit_secs: u64,
    subscriptions: &mut Subscriptions,
) {
    let (update_tx, update_rx) =
        stateless::create_pipe::<SensorsUpdate>(&format!("open_epaper_link/{name}/updates"));
    let (display_tx, display_rx) =
        stateful::create_pipe::<SensorsState>(&format!("open_epaper_link/{name}/state"));

    for (index, sensor) in sensors.iter().enumerate() {
        let rx = subscriptions.subscribe_into_stateful::<Json<zwave::Data<f64>>>(&sensor.topic);
        let update_tx = update_tx.clone();
        rx.for_each(move |(_, Json(data))| {
            update_tx.try_send(SensorsUpdate { index, data });
        });
    }

    let count = sensors.len();
    spawn(async move {
        let mut sub = update_rx.subscribe().await;
        let mut state = SensorsState::new(count);

        while let Ok(update) = sub.recv().await {
            state.update(update.index, &update.data);
            display_tx.try_send(state.clone());
        }
    });

    let rx = display_rx.rate_limit(
        &format!("open_epaper_link/{name}/ratelimit"),
        Duration::from_secs(rate_limit_secs),
    );

    let labels: Vec<(String, Option<String>)> = sensors
        .iter()
        .map(|sensor| (sensor.label.clone(), sensor.unit.clone()))
        .collect();

    let device = Arc::new(device);
    let name = name.to_string();
    rx.async_for_each(move |(_, state)| {
        let device = device.clone();
        let name = name.clone();
        let labels = labels.clone();
        async move {
            let template = render_sensors(&name, &labels, &state, timezone);
            if let Err(e) = template.send(&device).await {
                error!("Failed to send template: {e}");
            }
        }
    });
}

/// A sensor reading update event.
#[derive(Clone, Debug)]
struct SensorsUpdate {
    /// The index of the sensor in the configured list.
    index: usize,

    /// The reading.
    data: zwave::Data<f64>,
}

/// Render the sensor display template.
///
/// Uses the compact fonts ([`HEADER_FONT`], [`TEXT_FONT`]) so all lines
/// fit a 152x152 tag; the display fits up to five sensors plus the
/// update line.
fn render_sensors(
    name: &str,
    labels: &[(String, Option<String>)],
    state: &SensorsState,
    timezone: Tz,
) -> Template {
    let mut y = 4;
    let mut template = Template(vec![header(&mut y, name.to_string()), line(&mut y)]);

    for ((label, unit), reading) in labels.iter().zip(state.readings.iter()) {
        let content = reading.as_ref().map_or_else(
            || format!("{label}: --"),
            |reading| {
                format!(
                    "{label}: {}{}",
                    reading.value,
                    unit.as_deref().unwrap_or("")
                )
            },
        );
        template.0.push(text(&mut y, content));
    }

    if let Some(last_update) = state.last_update() {
        let local = last_update.with_timezone(&timezone);
        template.0.push(line(&mut y));
        template.0.push(text(
            &mut y,
            format!("Upd: {}", local.format("%H:%M %d %b")),
        ));
    }

    template
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_serialize_template() {
        let template = Template(vec![
            Element::Text(Text {
                x: 0,
                y: 0,
                content: "Hello, World!".to_string(),
                font: "Arial".to_string(),
                color: 0,
            }),
            Element::Box(Box {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
                color: 0,
            }),
            Element::Line(Line {
                x1: 0,
                y1: 0,
                x2: 100,
                y2: 100,
                color: 0,
            }),
        ]);
        let serialized = serde_json::to_string(&template).unwrap();
        assert_eq!(
            serialized,
            r#"[{"text":[0,0,"Hello, World!","Arial",0]},{"box":[0,0,100,100,0]},{"line":[0,0,100,100,0]}]"#
        );
    }

    #[test]
    fn test_config_deserialize() {
        let yaml = r#"
            url: "http://oel.example.com/"
            timezone: "Australia/Sydney"
            displays:
              - type: locations
                mac: "00000000abcdef"
                id: "brian-tesla"
              - type: sensors
                mac: "00000000abcdee"
                name: "Outside Front"
                rate_limit_secs: 600
                sensors:
                  - label: "Temp"
                    topic: "zwave/Outside_Front/Outside_Front_Environment/49/0/Temperature"
                    unit: "°C"
                  - label: "Humidity"
                    topic: "zwave/Outside_Front/Outside_Front_Environment/49/0/Humidity"
        "#;
        let config: Config = serde_yaml_ng::from_str(yaml).unwrap();

        assert_eq!(config.timezone.to_string(), "Australia/Sydney");
        assert_eq!(config.displays.len(), 2);

        match &config.displays[0] {
            DisplayConfig::Locations {
                id,
                rate_limit_secs,
                ..
            } => {
                assert_eq!(id.to_id_string(), "brian-tesla");
                assert_eq!(*rate_limit_secs, 300);
            }
            DisplayConfig::Sensors { .. } => panic!("expected a locations display"),
        }

        match &config.displays[1] {
            DisplayConfig::Sensors {
                name,
                sensors,
                rate_limit_secs,
                ..
            } => {
                assert_eq!(name, "Outside Front");
                assert_eq!(*rate_limit_secs, 600);
                assert_eq!(sensors.len(), 2);
                assert_eq!(sensors[0].label, "Temp");
                assert_eq!(sensors[0].unit.as_deref(), Some("°C"));
                assert_eq!(sensors[1].unit, None);
            }
            DisplayConfig::Locations { .. } => panic!("expected a sensors display"),
        }
    }

    #[test]
    fn test_config_deserialize_defaults() {
        let yaml = r#"
            url: "http://oel.example.com/"
            displays: []
        "#;
        let config: Config = serde_yaml_ng::from_str(yaml).unwrap();

        assert_eq!(config.timezone.to_string(), "UTC");
        assert!(config.displays.is_empty());
    }

    #[test]
    fn test_render_sensors() {
        let timezone: Tz = "Australia/Sydney".parse().unwrap();
        let labels = vec![
            ("Temp".to_string(), Some("°C".to_string())),
            ("Humidity".to_string(), None),
        ];

        let mut state = SensorsState::new(2);
        let template = render_sensors("Outside Front", &labels, &state, timezone);
        let Element::Text(Text { content, .. }) = &template.0[2] else {
            panic!("expected text");
        };
        assert_eq!(content, "Temp: --");
        assert_eq!(state.last_update(), None);
        // Without readings there is no separator or update line.
        assert_eq!(template.0.len(), 4);

        let temp: zwave::Data<f64> =
            serde_json::from_str(r#"{"time":1795140178204,"value":17.6}"#).unwrap();
        state.update(0, &temp);
        let humidity: zwave::Data<f64> =
            serde_json::from_str(r#"{"time":1795140178743,"value":81}"#).unwrap();
        state.update(1, &humidity);

        let template = render_sensors("Outside Front", &labels, &state, timezone);
        let Element::Text(Text { content, font, .. }) = &template.0[2] else {
            panic!("expected text");
        };
        assert_eq!(content, "Temp: 17.6°C");
        assert_eq!(font, TEXT_FONT);
        let Element::Text(Text { content, .. }) = &template.0[3] else {
            panic!("expected text");
        };
        assert_eq!(content, "Humidity: 81");
        let Element::Text(Text { content, y, .. }) = &template.0[5] else {
            panic!("expected text");
        };
        assert_eq!(content, "Upd: 13:02 20 Nov");
        // The update line of a three sensor display fits the 152 px tag height.
        assert!(*y + ROW_HEIGHT <= 152);
    }

    #[test]
    fn test_sensors_state_out_of_range() {
        let mut state = SensorsState::new(1);
        let data: zwave::Data<f64> =
            serde_json::from_str(r#"{"time":1795140178204,"value":1.0}"#).unwrap();
        state.update(7, &data);
        assert_eq!(state.readings[0], None);
    }
}
