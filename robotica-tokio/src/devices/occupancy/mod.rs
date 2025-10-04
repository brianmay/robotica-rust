//! Module for handling occupancy or PIR sensors
use robotica_common::mqtt::Json;
use serde::{Deserialize, Serialize};

use crate::{pipes::stateful, services::mqtt::Subscriptions};

/// The type of sensor
#[derive(Deserialize, Debug, Clone)]
pub enum SensorType {
    /// Zigbee sensor
    Zigbee,
    /// Zwave sensor
    Zwave,
}

/// The configuration for an occupancy sensor
#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    /// The type of sensor
    sensor_type: SensorType,
    /// The MQTT topic to subscribe to for occupancy messages
    topic: String,
}

/// The occupancy state
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum OccupiedState {
    /// The space is occupied
    Occupied,
    /// The space is vacant
    Vacant,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ZigbeeMessage {
    occupancy: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    }
}
