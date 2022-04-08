use std::time::Duration;

use paho_mqtt::Message;
use robotica_node_rust::sources::mqtt::{MqttOut, Subscriptions};
use serde::Deserialize;

use super::robotica::RoboticaLightCommand;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct Beacon {
    id: String,
    id_type: u32,
    rssi: i32,
    raw: f32,
    distance: f32,
    speed: f32,
    mac: String,
    interval: Option<u32>,
}

pub fn start(subscriptions: &mut Subscriptions, mqtt_out: &MqttOut) {
    subscriptions
        .subscribe_to_string(
            "espresense/devices/iBeacon:63a1368d-552b-4ea3-aed5-b5fefb2adf09-99-86/brian",
        )
        .map(|s| {
            let b: Beacon = serde_json::from_str(&s).unwrap();
            b
        })
        .map(|b| b.distance < 20.0)
        .debug("Brian is in bedroom")
        .startup_delay(Duration::from_secs(10), false)
        .delay_cancel(Duration::from_secs(10))
        .debug("Brian is in bedroom (delayed)")
        .diff()
        .changed()
        .debug("Brian is in bedroom (changed)")
        .map(|v| {
            let action = if v {
                None
            } else {
                Some("turn_off".to_string())
            };
            let command = RoboticaLightCommand {
                action,
                color: None,
                scene: Some("auto".to_string()),
            };
            let location = "Brian";
            let device = "Light";
            let topic = format!("command/{location}/{device}");
            let payload = serde_json::to_string(&command).unwrap();
            Message::new(topic, payload, 0)
        })
        .publish(mqtt_out);
}
