use std::time::Duration;

use paho_mqtt::Message;
use robotica_node_rust::{
    filters::{ChainGeneric, ChainTimer},
    sources::{
        mqtt::{MqttMessage, Subscriptions},
        ChainMqtt,
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{Receiver, Sender};

pub trait ChainMessage {
    fn message(self, subscriptions: &mut Subscriptions, mqtt_out: &Sender<MqttMessage>);
}

impl ChainMessage for Receiver<String> {
    fn message(
        self,
        subscriptions: &mut Subscriptions,
        mqtt_out: &tokio::sync::mpsc::Sender<MqttMessage>,
    ) {
        message(self, subscriptions, mqtt_out)
    }
}

fn power_to_bool(value: String) -> bool {
    matches!(value.as_str(), "ON")
}

#[derive(Serialize, Deserialize, Debug)]
struct MessageText {
    text: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct AudioMessage {
    message: MessageText,
}

fn string_to_message(str: String, topic: &str) -> Message {
    let msg = AudioMessage {
        message: MessageText { text: str },
    };
    // let topic = "command/Brian/Robotica";
    let payload = serde_json::to_string(&msg).unwrap();
    Message::new(topic, payload, 0)
}

pub fn message_location(
    rx: Receiver<String>,
    subscriptions: &mut Subscriptions,
    mqtt: &Sender<MqttMessage>,
    location: &str,
) {
    let gate_topic = format!("state/{}/Messages/power", location);
    let command_topic = format!("command/{}/Robotica", location);

    let do_gate = subscriptions
        .subscribe(&gate_topic)
        .map(power_to_bool)
        .debug(format!("gate1 {location}"))
        .delay_true(Duration::from_secs(5))
        .debug(format!("gate2 {location}"))
        .timer(Duration::from_secs(1))
        .debug(format!("gate3 {location}"));

    rx.gate(do_gate)
        .map(move |v| string_to_message(v, &command_topic))
        .publish(mqtt.clone());
}

pub fn message(
    rx: Receiver<String>,
    subscriptions: &mut Subscriptions,
    mqtt: &Sender<MqttMessage>,
) {
    message_location(rx, subscriptions, mqtt, "Brian");
}
