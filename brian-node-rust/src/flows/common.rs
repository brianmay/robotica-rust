use log::*;
use paho_mqtt::Message;
use robotica_node_rust::{
    filters::{ChainDebug, ChainGeneric, ChainSplit},
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

pub fn power_to_bool(value: String) -> bool {
    value == "ON"
}

pub fn string_to_integer(str: String) -> Option<usize> {
    match str.parse::<usize>() {
        Ok(value) => Some(value),
        Err(_) => {
            error!("Invalid integer {str} received");
            None
        }
    }
}

pub fn string_to_bool(str: String) -> Option<bool> {
    match str.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        str => {
            error!("Invalid bool {str} received");
            None
        }
    }
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

    let do_gate = subscriptions.subscribe(&gate_topic).map(power_to_bool);

    rx.gate(do_gate)
        .map(move |v| string_to_message(v, &command_topic))
        .publish(mqtt.clone());
}

pub fn message(
    rx: Receiver<String>,
    subscriptions: &mut Subscriptions,
    mqtt: &Sender<MqttMessage>,
) {
    let rx = rx.debug("outgoing message").split();
    message_location(rx.receiver(), subscriptions, mqtt, "Brian");
    message_location(rx.receiver(), subscriptions, mqtt, "Dining");
}
