use std::time::Duration;

use paho_mqtt::Message;
use robotica_node_rust::{
    sources::mqtt::{MqttOut, Subscriptions},
    TxPipe,
};
use serde::Deserialize;

use super::robotica::{Id, RoboticaLightCommand};

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ThirdRealityDoorSensor {
    battery: Option<u8>,
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

pub fn start(subscriptions: &mut Subscriptions, message_sink: &TxPipe<String>, mqtt_out: &MqttOut) {
    subscriptions
        .subscribe_to_string("zigbee2mqtt/Dining/door")
        .filter_map(third_reality_door_sensor)
        .map(|door_sensor| !door_sensor.contact)
        .delay_true(Duration::from_secs(30))
        .diff_with_initial_value(Some(false))
        .changed()
        .timer_true(Duration::from_secs(60))
        .map(|state| match state {
            true => "Please close the front door".to_string(),
            false => "Thank-you for closing the front door".to_string(),
        })
        .copy_to(message_sink);

    subscriptions
        .subscribe_to_string("zigbee2mqtt/Bathroom/door")
        .filter_map(third_reality_door_sensor)
        .map(|door_sensor| !door_sensor.contact)
        .diff()
        .changed()
        .map(|contact| match contact {
            false => RoboticaLightCommand {
                action: None,
                color: None,
                scene: Some("alert_bathroom".to_string()),
            },
            true => RoboticaLightCommand {
                action: Some("turn_off".to_string()),
                color: None,
                scene: Some("alert_bathroom".to_string()),
            },
        })
        .map(|command| {
            let id = Id::new("Bathroom", "Light");
            let topic = id.get_command_topic(&[]);
            let payload = serde_json::to_string(&command).unwrap();
            Message::new(topic, payload, 0)
        })
        .publish(mqtt_out);
}
