//! Module for handling occupancy or PIR sensors
pub use robotica_common::robotica::occupancy::OccupiedState;

use robotica_common::mqtt::Json;
use serde::Deserialize;

use crate::{pipes::stateful, services::mqtt::Subscriptions};

/// The type of sensor
#[derive(Deserialize, Debug, Clone)]
pub enum SensorType {
    /// Zigbee sensor
    Zigbee,
    /// Zwave sensor
    Zwave,
    /// MSR-2 radar sensor (publishes `ON`/`OFF` string payloads)
    Msr2,
}

/// The configuration for an occupancy sensor
#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    /// The type of sensor
    sensor_type: SensorType,
    /// The MQTT topic to subscribe to for occupancy messages
    topic: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
struct ZigbeeMessage {
    occupancy: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
struct ZwaveMessage {
    value: u8,
}

impl From<ZigbeeMessage> for OccupiedState {
    fn from(msg: ZigbeeMessage) -> Self {
        if msg.occupancy {
            OccupiedState::Occupied
        } else {
            OccupiedState::Vacant
        }
    }
}
impl From<ZwaveMessage> for OccupiedState {
    fn from(msg: ZwaveMessage) -> Self {
        if msg.value > 0 {
            OccupiedState::Occupied
        } else {
            OccupiedState::Vacant
        }
    }
}

/// Payload from an MSR-2 radar sensor, which publishes the plain strings
/// `ON` (presence detected) or `OFF` (no presence).
#[derive(Clone, Debug, Eq, PartialEq)]
struct Msr2Message(String);

impl From<Msr2Message> for OccupiedState {
    fn from(msg: Msr2Message) -> Self {
        match msg.0.as_str() {
            "ON" => OccupiedState::Occupied,
            _ => OccupiedState::Vacant,
        }
    }
}

/// Subscribe to occupancy messages and return a stateful receiver of occupancy state.
pub fn subscribe(
    config: &Config,
    subscriptions: &mut Subscriptions,
) -> stateful::Receiver<OccupiedState> {
    match config.sensor_type {
        SensorType::Zigbee => {
            let rx: stateful::Receiver<Json<ZigbeeMessage>> =
                subscriptions.subscribe_into_stateful(&config.topic);
            rx.map(|(_, msg)| msg.0.into())
        }
        SensorType::Zwave => {
            let rx: stateful::Receiver<Json<ZwaveMessage>> =
                subscriptions.subscribe_into_stateful(&config.topic);
            rx.map(|(_, msg)| msg.0.into())
        }
        SensorType::Msr2 => {
            let rx: stateful::Receiver<String> =
                subscriptions.subscribe_into_stateful(&config.topic);
            rx.map(|(_, msg)| Msr2Message(msg).into())
        }
    }
}
