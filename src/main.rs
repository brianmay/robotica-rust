use anyhow::Result;
use log::*;
use mqtt::Message;
use paho_mqtt as mqtt;
use serde::{Deserialize, Serialize};
use std::str;
use tokio::sync::mpsc;

use robotica_nodes_rust::sources::mqtt::{Subscriptions, Mqtt, MqttMessage};

fn has_changed<T: Send + Eq + Clone + 'static>(
    mut input: mpsc::Receiver<T>,
) -> mpsc::Receiver<(T, T)> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        let mut old_value: Option<T> = None;
        while let Some(v) = input.recv().await {
            if let Some(prev) = old_value {
                if prev != v {
                    let v_clone = v.clone();
                    let a = tx.send((prev, v_clone)).await;
                    a.unwrap_or_else(|err| {
                        error!("send operation failed {err}");
                    });
                }
            };
            old_value = Some(v);
        }
    });

    rx
}

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

fn changed_to_string(value: (Power, Power)) -> Option<String> {
    match value {
        (Power::Error, _) => None,
        (_, Power::Error) => None,
        (_, Power::Off) => Some("Fan has been turned off".to_string()),
        (_, Power::On) => Some("Fan has been turned on".to_string()),
        (_, Power::HardOff) => Some("Fan has been turned off at power point".to_string()),
    }
}

fn string_to_message(str: String) -> Message {
    let msg = AudioMessage {
        message: MessageText { text: str },
    };
    let topic = "command/Brian/Robotica";
    let payload = serde_json::to_string(&msg).unwrap();
    Message::new(topic, payload, 0)
}

fn map<T: Send + core::fmt::Debug + 'static, U: Send + core::fmt::Debug + 'static>(
    mut input: mpsc::Receiver<T>,
    callback: fn(T) -> U,
) -> mpsc::Receiver<U> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        while let Some(v) = input.recv().await {
            println!("map {v:?}");
            let v = callback(v);
            println!("--> {v:?}");
            tx.send(v).await.unwrap();
        }
    });

    rx
}

fn filter_map<T: Send + core::fmt::Debug + 'static, U: Send + core::fmt::Debug + 'static>(
    mut input: mpsc::Receiver<T>,
    callback: fn(T) -> Option<U>,
) -> mpsc::Receiver<U> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        while let Some(v) = input.recv().await {
            println!("filter_map {v:?}");
            if let Some(v) = callback(v) {
                println!("--> {v:?}");
                tx.send(v).await.unwrap();
            }
        }
    });

    rx
}

fn filter<T: Send + core::fmt::Debug + 'static>(
    mut input: mpsc::Receiver<T>,
    callback: fn(&T) -> bool,
) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        while let Some(v) = input.recv().await {
            let filter = callback(&v);
            if filter {
                tx.send(v).await.unwrap();
            }
        }
    });

    rx
}

async fn publish(mut input: mpsc::Receiver<Message>, mqtt_out: mpsc::Sender<MqttMessage>) {
    tokio::spawn(async move {
        while let Some(v) = input.recv().await {
            // let msg = Message::new("test", v, 0);
            mqtt_out.send(MqttMessage::MqttOut(v)).await.unwrap();
        }
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut mqtt = Mqtt::new().await;
    let tx = mqtt.take_tx()?;

    let subscriptions: Subscriptions = setup_pipes(&tx).await;
    mqtt.connect(subscriptions);

    drop(mqtt);
    Ok(())
}


async fn setup_pipes(main_tx: &mpsc::Sender<MqttMessage>) -> Subscriptions {
    let mut subscriptions: Subscriptions = Subscriptions::new();

    let rx = subscriptions.subscription("state/Brian/Fan/power");
    let rx = map(rx, power_to_enum);
    let rx = has_changed(rx);
    let rx = filter_map(rx, changed_to_string);
    let rx = map(rx, string_to_message);
    publish(rx, main_tx.clone()).await;

    subscriptions
}
