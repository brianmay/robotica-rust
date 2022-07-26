use std::time::Duration;

use paho_mqtt::Message;
use robotica_node_rust::{
    sources::mqtt::{MqttOut, Subscriptions},
    RxPipe, TxPipe,
};
use serde::Deserialize;

use super::{
    common::power_to_bool,
    robotica::{string_to_message, Action, Id, RoboticaDeviceCommand, RoboticaLightCommand},
};

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

pub fn bathroom_location(
    rx: RxPipe<bool>,
    subscriptions: &mut Subscriptions,
    mqtt: &MqttOut,
    location: &str,
) {
    let gate_id = Id::new(location, "Request_Bathroom");
    let gate_topic = gate_id.get_state_topic("power");
    let gate_command_topic = gate_id.get_command_topic(&[]);
    let command_id = Id::new(location, "Robotica");
    let command_topic = command_id.get_command_topic(&[]);

    let do_gate = subscriptions
        .subscribe_to_string(&gate_topic)
        .map(power_to_bool);

    let gate_result = rx.gate(do_gate);

    gate_result
        .filter(move |msg| *msg)
        .map(|_| RoboticaDeviceCommand {
            action: Some(Action::TurnOff),
        })
        .map(move |command| {
            let payload = serde_json::to_string(&command).unwrap();
            Message::new(gate_command_topic.clone(), payload, 0)
        })
        .publish(mqtt);

    gate_result
        .map(|contact| match contact {
            // Door is open
            true => "The bathroom is free".to_string(),
            // Door is closed
            false => "The bathroom is occupied".to_string(),
        })
        .map(move |v| string_to_message(v, &command_topic))
        .publish(mqtt);
}

pub fn start(
    subscriptions: &mut Subscriptions,
    _message_sink: &TxPipe<String>,
    mqtt_out: &MqttOut,
) {
    let bathroom = subscriptions
        .subscribe_to_string("zigbee2mqtt/Bathroom/door")
        .filter_map(third_reality_door_sensor)
        .map(|door_sensor| !door_sensor.contact)
        .diff()
        .changed()
        .debug("bathroom door open");

    bathroom
        .map(|contact| match contact {
            // Door is open
            true => RoboticaLightCommand {
                action: Some(Action::TurnOff),
                color: None,
                scene: Some("alert_bathroom".to_string()),
            },
            //Door is closed
            false => RoboticaLightCommand {
                action: None,
                color: None,
                scene: Some("alert_bathroom".to_string()),
            },
        })
        .map(|command| {
            let id = Id::new("Passage", "Light");
            let topic = id.get_command_topic(&[]);
            let payload = serde_json::to_string(&command).unwrap();
            Message::new(topic, payload, 0)
        })
        .publish(mqtt_out);

    bathroom_location(bathroom.clone(), subscriptions, mqtt_out, "Brian");
    bathroom_location(bathroom, subscriptions, mqtt_out, "Dining");
}
