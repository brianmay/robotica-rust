use anyhow::Result;
use log::*;
use paho_mqtt::AsyncClient;
use paho_mqtt::ConnectOptionsBuilder;
use paho_mqtt::CreateOptionsBuilder;
use paho_mqtt::Message;
use paho_mqtt::SslOptionsBuilder;
use std::cmp::min;
use std::collections::HashMap;
use std::time::Duration;
use std::{env, str};
use tokio::select;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio::time::timeout;

use crate::send;
use crate::spawn;
use crate::PIPE_SIZE;

#[derive(Debug)]
pub enum MqttMessage {
    MqttOut(Message),
}

pub struct Mqtt {
    b: Option<JoinHandle<()>>,
    rx: Option<mpsc::Receiver<MqttMessage>>,
    tx: Option<mpsc::Sender<MqttMessage>>,
}

impl Mqtt {
    pub async fn new() -> Self {
        let (main_tx, main_rx) = mpsc::channel(PIPE_SIZE);

        Mqtt {
            b: None,
            rx: Some(main_rx),
            tx: Some(main_tx),
        }
    }

    pub fn take_tx(&mut self) -> Result<mpsc::Sender<MqttMessage>> {
        self.tx
            .take()
            .ok_or_else(|| anyhow::anyhow!("tx value taken"))
    }

    pub fn connect(&mut self, subscriptions: Subscriptions) {
        // Define the set of options for the create.
        // Use an ID for a persistent session.
        let uri = format!(
            "ssl://{}:{}",
            env::var("MQTT_HOST").expect("MQTT_HOST should be set"),
            env::var("MQTT_PORT").expect("MQTT_PORT should be set")
        );

        let hostname = gethostname::gethostname();
        let hostname = hostname.to_str().unwrap();
        let client_id = format!("robotica-node-rust-{hostname}");

        let create_opts = CreateOptionsBuilder::new()
            .server_uri(&uri)
            .client_id(client_id) // FIXME: This is bad
            .finalize();

        // Create a client.
        let mut cli = AsyncClient::new(create_opts).unwrap_or_else(|err| {
            panic!("Error creating the client to {uri}: {:?}", err);
        });

        let mqtt_in_rx = cli.get_stream(10);

        let rx = self.rx.take().unwrap();
        let b = spawn(async move {
            let trust_store = env::var("MQTT_CA_CERT_FILE").unwrap();

            let ssl_opts = SslOptionsBuilder::new()
                .trust_store(trust_store)
                .unwrap()
                .finalize();

            // Define the set of options for the connection.
            let conn_opts = ConnectOptionsBuilder::new()
                .ssl_options(ssl_opts)
                .keep_alive_interval(Duration::from_secs(30))
                .clean_session(true)
                .user_name(env::var("MQTT_USERNAME").expect("MQTT_USERNAME should be set"))
                .password(env::var("MQTT_PASSWORD").expect("MQTT_PASSWORD should be set"))
                .finalize();

            // Connect and wait for it to complete or fail.
            if let Err(e) = cli.connect(conn_opts).await {
                panic!("Unable to connect to {uri}:\n\t{:?}", e);
            }

            let mut rx = rx;

            // Subscribe topics.
            subscribe_topics(&cli, &subscriptions).await;

            loop {
                select! {
                    Ok(msg_or_none) = mqtt_in_rx.recv() => {
                        if let Some(msg) = msg_or_none {
                            let topic = msg.topic();
                            let payload = msg.payload();
                            let payload = str::from_utf8(payload).unwrap().to_string();
                            debug!("incoming mqtt {topic} {payload}");
                            for subscription in subscriptions.get(topic) {
                                send(&subscription.tx, payload.clone()).await;
                            }
                        } else if !cli.is_connected() {
                            try_reconnect(&cli).await;
                            debug!("Resubscribe topics...");
                            subscribe_topics(&cli, &subscriptions).await;
                        }
                    },
                    Some(msg) = rx.recv() => {
                        match msg {
                            MqttMessage::MqttOut(msg) => cli.publish(msg).await.unwrap(),
                        }
                    }
                    else => { break; }
                };
            }
        });

        self.b = Some(b);
    }
}

impl Mqtt {
    pub async fn wait(&mut self) {
        if let Some(b) = self.b.take() {
            b.await.unwrap();
        }
    }
}

#[derive(Clone)]
struct Subscription {
    #[allow(dead_code)]
    topic: String,
    tx: mpsc::Sender<String>,
}

pub struct Subscriptions(HashMap<String, Vec<Subscription>>);

impl Subscriptions {
    pub fn new() -> Self {
        Subscriptions(HashMap::new())
    }

    fn get(&self, topic: &str) -> Vec<Subscription> {
        match self.0.get(topic) {
            Some(list) => (*list).clone(),
            None => vec![],
        }
    }

    pub fn subscribe(&mut self, topic: &str) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel(PIPE_SIZE);
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

impl Default for Subscriptions {
    fn default() -> Self {
        Self::new()
    }
}

async fn try_reconnect(cli: &AsyncClient) {
    let mut attempt: u32 = 0;
    loop {
        let sleep_time = 1000 * 2u64.checked_pow(attempt).unwrap();
        let sleep_time = min(60_000, sleep_time);

        warn!("Connection lost to mqtt. Waiting {sleep_time} ms to retry connection attempt {attempt}.");
        sleep(Duration::from_millis(sleep_time)).await;

        warn!("Trying to connect to mqtt");
        match timeout(Duration::from_secs(10), cli.reconnect()).await {
            Ok(result) => {
                if result.is_ok() {
                    warn!("Successfully reconnected to mqtt");
                    break;
                } else {
                    error!("Reconnect failed");
                }
            }
            Err(timeout) => error!("Timeout trying to reconnect {timeout}"),
        }

        attempt = attempt.saturating_add(1);
    }
}

async fn subscribe_topics(cli: &AsyncClient, subscriptions: &Subscriptions) {
    let topics: Vec<_> = subscriptions
        .0
        .iter()
        .map(|(topic, _)| topic.clone())
        .collect();
    let qos: Vec<_> = subscriptions.0.iter().map(|_| 0).collect();

    if let Err(e) = cli.subscribe_many(&topics, &qos).await {
        error!("Error subscribes topics: {:?}", e);
    }
}

pub fn publish(mut input: mpsc::Receiver<Message>, mqtt_out: mpsc::Sender<MqttMessage>) {
    spawn(async move {
        while let Some(v) = input.recv().await {
            let debug_mode: bool = match env::var("DEBUG_MODE") {
                Ok(value) => value.to_lowercase() == "true",
                Err(_) => false,
            };

            info!(
                "outgoing mqtt {} {} {} {}",
                if debug_mode { "nop" } else { "live" },
                v.retained(),
                v.topic(),
                str::from_utf8(v.payload()).unwrap().to_string()
            );

            if !debug_mode {
                send(&mqtt_out, MqttMessage::MqttOut(v)).await;
            }
        }
    });
}
