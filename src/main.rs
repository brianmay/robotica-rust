use anyhow::Result;
use log::*;
use mqtt::Message;
use paho_mqtt as mqtt;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, str, thread, time::Duration};
use tokio::sync::mpsc;

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

fn try_reconnect(cli: &mqtt::Client) -> bool {
    println!("Connection lost. Waiting to retry connection");
    for _ in 0..12 {
        thread::sleep(Duration::from_millis(5000));
        if cli.reconnect().is_ok() {
            println!("Successfully reconnected");
            return true;
        }
    }
    println!("Unable to reconnect after several attempts.");
    false
}

fn subscribe_topics(cli: &mqtt::Client, subscriptions: &Subscriptions) {
    let topics: Vec<_> = subscriptions
        .0
        .iter()
        .map(|(topic, _)| topic.clone())
        .collect();
    let qos: Vec<_> = subscriptions.0.iter().map(|_| 0).collect();

    if let Err(e) = cli.subscribe_many(&topics, &qos) {
        error!("Error subscribes topics: {:?}", e);
    }
}

#[derive(Debug)]
enum MqttMessage {
    MqttIn(Option<Message>),
    MqttOut(Message),
}

#[derive(Clone)]
struct Subscription {
    topic: String,
    tx: mpsc::Sender<String>,
}

struct Subscriptions(HashMap<String, Vec<Subscription>>);

impl Subscriptions {
    fn new() -> Self {
        Subscriptions(HashMap::new())
    }

    fn get(&self, topic: &str) -> Vec<Subscription> {
        match self.0.get(topic) {
            Some(list) => (*list).clone(),
            None => vec![],
        }
    }

    fn subscription(&mut self, topic: &str) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel(10);
        let subscription = Subscription {
            topic: topic.to_string(),
            tx,
        };
        if let Some(list) = self.0.get_mut(topic) {
            list.push(subscription);
        } else {
            self.0.insert(topic.to_string(), vec![subscription]);
        }
        rx
    }
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

struct Mqtt {
    a: Option<thread::JoinHandle<()>>,
    b: Option<thread::JoinHandle<()>>,
    rx: Option<mpsc::Receiver<MqttMessage>>,
    tx: Option<mpsc::Sender<MqttMessage>>,
    tx_private: Option<mpsc::Sender<MqttMessage>>,
}

impl Mqtt {
    async fn new() -> Self {
        let (main_tx, main_rx) = mpsc::channel(10);

        Mqtt {
            a: None,
            b: None,
            rx: Some(main_rx),
            tx: Some(main_tx.clone()),
            tx_private: Some(main_tx),
        }
    }

    fn take_tx(&mut self) -> Result<mpsc::Sender<MqttMessage>> {
        self.tx
            .take()
            .ok_or_else(|| anyhow::anyhow!("tx value taken"))
    }

    fn connect(&mut self, subscriptions: Subscriptions) {
        // Define the set of options for the create.
        // Use an ID for a persistent session.
        let uri = format!(
            "ssl://{}:{}",
            env::var("MQTT_HOST").unwrap(),
            env::var("MQTT_PORT").unwrap()
        );

        let create_opts = mqtt::CreateOptionsBuilder::new()
            .server_uri(&uri)
            .client_id("rust-nodes".to_string())
            .finalize();

        // Create a client.
        let mut cli = mqtt::Client::new(create_opts).unwrap_or_else(|err| {
            panic!("Error creating the client to {uri}: {:?}", err);
        });

        let mqtt_in_rx = cli.start_consuming();
        let tx = self.tx_private.clone().unwrap();
        let a = thread::spawn(move || {
            for msg_or_none in mqtt_in_rx.iter() {
                tx.blocking_send(MqttMessage::MqttIn(msg_or_none)).unwrap();
            }
        });

        let rx = self.rx.take().unwrap();
        let b = thread::spawn(move || {
            let trust_store = env::var("MQTT_CA_CERT_FILE").unwrap();

            let ssl_opts = mqtt::SslOptionsBuilder::new()
                .trust_store(trust_store)
                .unwrap()
                .finalize();

            // Define the set of options for the connection.
            let conn_opts = mqtt::ConnectOptionsBuilder::new()
                .ssl_options(ssl_opts)
                .keep_alive_interval(Duration::from_secs(30))
                .clean_session(true)
                .user_name(env::var("MQTT_USER_NAME").unwrap())
                .password(env::var("MQTT_PASSWORD").unwrap())
                .finalize();

            // Connect and wait for it to complete or fail.
            if let Err(e) = cli.connect(conn_opts) {
                panic!("Unable to connect to {uri}:\n\t{:?}", e);
            }

            let mut rx = rx;

            // Subscribe topics.
            subscribe_topics(&cli, &subscriptions);

            while let Some(msg) = rx.blocking_recv() {
                let msg: MqttMessage = msg;
                match msg {
                    MqttMessage::MqttIn(msg_or_none) => {
                        if let Some(msg) = msg_or_none {
                            let topic = msg.topic();
                            let payload = msg.payload();
                            let payload = str::from_utf8(payload).unwrap().to_string();
                            for subscription in subscriptions.get(topic) {
                                subscription.tx.blocking_send(payload.clone()).unwrap();
                            }
                        } else if !cli.is_connected() {
                            if try_reconnect(&cli) {
                                println!("Resubscribe topics...");
                                subscribe_topics(&cli, &subscriptions);
                            } else {
                                break;
                            };
                        }
                    }
                    MqttMessage::MqttOut(msg) => cli.publish(msg).unwrap(),
                }
            }
        });

        self.a = Some(a);
        self.b = Some(b);
    }
}

impl Drop for Mqtt {
    fn drop(&mut self) {
        if let Some(a) = self.a.take() {
            a.join().unwrap();
        }
        if let Some(b) = self.b.take() {
            b.join().unwrap();
        }
    }
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
