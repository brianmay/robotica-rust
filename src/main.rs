use anyhow::Result;
use mqtt::Message;
use paho_mqtt as mqtt;
use serde::{Deserialize, Serialize};
use std::str;
use tokio::sync::mpsc::{self, Receiver};

use robotica_nodes_rust::{
    filters::generic::{debug, filter_map, gate, has_changed, map},
    sources::mqtt::{publish, Mqtt, MqttMessage, Subscriptions},
};

#[derive(Serialize, Deserialize, Debug)]
struct MessageText {
    text: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct AudioMessage {
    message: MessageText,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Power {
    On,
    Off,
    HardOff,
    Error,
}

fn power_to_enum(value: String) -> Power {
    match value.as_str() {
        "OFF" => Power::Off,
        "ON" => Power::On,
        "HARD_OFF" => Power::HardOff,
        _ => Power::Error,
    }
}

fn power_to_bool(value: String) -> bool {
    matches!(value.as_str(), "ON")
}

fn changed_to_string(value: (Power, Power)) -> Option<String> {
    match value {
        (Power::Error, _) => None,
        (_, Power::Error) => None,
        (_, Power::Off) => Some("Fan has been turned off".to_string()),
        (_, Power::On) => Some("Fan has been turned on".to_string()),
        (_, Power::HardOff) => Some("Fan has been turned off at power point".to_string()),
    }
}

fn string_to_message(str: String, topic: &str) -> Message {
    let msg = AudioMessage {
        message: MessageText { text: str },
    };
    // let topic = "command/Brian/Robotica";
    let payload = serde_json::to_string(&msg).unwrap();
    Message::new(topic, payload, 0)
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut mqtt = Mqtt::new().await;
    let tx = mqtt.take_tx()?;

    let subscriptions: Subscriptions = setup_pipes(&tx);
    mqtt.connect(subscriptions);

    drop(mqtt);
    Ok(())
}

fn message_location(
    rx: Receiver<String>,
    subscriptions: &mut Subscriptions,
    mqtt: &mpsc::Sender<MqttMessage>,
    location: &str,
) {
    let gate_topic = format!("state/{}/Messages/power", location);
    let command_topic = format!("command/{}/Robotica", location);

    let do_gate = subscriptions.subscribe(&gate_topic);
    let do_gate = map(do_gate, power_to_bool);
    let do_gate = debug(do_gate, format!("gate {location}"));
    let rx = gate(rx, do_gate);
    let rx = map(rx, move |v| string_to_message(v, &command_topic));
    publish(rx, mqtt.clone());
}

fn message(
    rx: Receiver<String>,
    subscriptions: &mut Subscriptions,
    mqtt: &mpsc::Sender<MqttMessage>,
) {
    message_location(rx, subscriptions, mqtt, "Brian");
}

fn setup_pipes(mqtt: &mpsc::Sender<MqttMessage>) -> Subscriptions {
    let mut subscriptions: Subscriptions = Subscriptions::new();

    let rx = subscriptions.subscribe("state/Brian/Fan/power");
    let rx = map(rx, power_to_enum);
    let rx = has_changed(rx);
    let rx = filter_map(rx, changed_to_string);
    message(rx, &mut subscriptions, mqtt);

    subscriptions
}
