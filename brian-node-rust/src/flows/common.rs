use log::*;
use paho_mqtt::Message;
use robotica_node_rust::{
    sources::mqtt::{MqttOut, Subscriptions},
    Pipe, RxPipe, TxPipe,
};
use serde::{Deserialize, Serialize};

use super::robotica::Id;

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

pub fn string_to_message(str: String, topic: &str) -> Message {
    let msg = AudioMessage {
        message: MessageText { text: str },
    };
    let payload = serde_json::to_string(&msg).unwrap();
    Message::new(topic, payload, 0)
}

pub fn message_location(
    rx: RxPipe<String>,
    subscriptions: &mut Subscriptions,
    mqtt: &MqttOut,
    location: &str,
) {
    let gate_id = Id::new(location, "Messages");
    let gate_topic = gate_id.get_state_topic("power");
    let command_id = Id::new(location, "Robotica");
    let command_topic = command_id.get_command_topic(&[]);

    let do_gate = subscriptions
        .subscribe_to_string(&gate_topic)
        .map(power_to_bool);

    rx.gate(do_gate)
        .map(move |v| string_to_message(v, &command_topic))
        .publish(mqtt);
}

pub fn message_sink(subscriptions: &mut Subscriptions, mqtt: &MqttOut) -> TxPipe<String> {
    let pipe_start = Pipe::new();

    let pipe = pipe_start.to_rx_pipe().debug("outgoing message");
    message_location(pipe.clone(), subscriptions, mqtt, "Brian");
    message_location(pipe, subscriptions, mqtt, "Dining");

    pipe_start.to_tx_pipe()
}
