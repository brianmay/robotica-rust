use std::time::Duration;

use robotica_node_rust::{sources::mqtt::Subscriptions, TxPipe};
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ThirdRealityDoorSensor {
    battery: u8,
    battery_low: bool,
    contact: bool,
    linkquality: u8,
    tamper: bool,
    voltage: u32,
}

fn third_reality_door_sensor(str: String) -> Option<ThirdRealityDoorSensor> {
    let d = &mut serde_json::Deserializer::from_str(&str);
    let door_sensor: Result<ThirdRealityDoorSensor, _> = serde_path_to_error::deserialize(d);

    match door_sensor {
        Ok(door_sensor) => Some(door_sensor),
        Err(err) => {
            log::error!("third_reality_door_sensor: {err}");
            None
        }
    }
}

pub fn start(subscriptions: &mut Subscriptions, message_sink: &TxPipe<String>) {
    subscriptions
        .subscribe_to_string("zigbee2mqtt/Dining/door")
        .filter_map(third_reality_door_sensor)
        .map(|door_sensor| !door_sensor.contact)
        .diff()
        .changed()
        .delay_true(Duration::from_secs(30))
        .timer_true(Duration::from_secs(60))
        .map(|state| match state {
            true => "Please close the front door".to_string(),
            false => "Thank-you for closing the front door".to_string(),
        })
        .copy_to(message_sink);
}
