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
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio::time::timeout;
use tokio::time::Instant;

use crate::send_or_discard;
use crate::spawn;
use crate::Pipe;

#[derive(Debug)]
enum MqttMessage {
    MqttOut(Message, Instant),
}

pub struct Mqtt {
    b: Option<JoinHandle<()>>,
    rx: Option<mpsc::Receiver<MqttMessage>>,
    tx: mpsc::Sender<MqttMessage>,
}

#[derive(Clone)]
pub struct MqttOut(mpsc::Sender<MqttMessage>);

impl Mqtt {
    pub async fn new() -> Self {
        // Outgoing MQTT queue.
        let (main_tx, main_rx) = mpsc::channel(50);

        Mqtt {
            b: None,
            rx: Some(main_rx),
            tx: main_tx,
        }
    }

    pub fn get_mqtt_out(&mut self) -> MqttOut {
        MqttOut(self.tx.clone())
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
            .client_id(client_id)
            .finalize();

        // Create a client.
        let mut cli = AsyncClient::new(create_opts).unwrap_or_else(|err| {
            panic!("Error creating the client to {uri}: {:?}", err);
        });

        // Main incoming MQTT queue.
        let mqtt_in_rx = cli.get_stream(50);

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
                            if let Some(subscription) = subscriptions.get(topic) {
                                send_or_discard(&subscription.tx, payload.clone());
                            }
                        } else if !cli.is_connected() {
                            try_reconnect(&cli).await;
                            debug!("Resubscribe topics...");
                            subscribe_topics(&cli, &subscriptions).await;
                        }
                    },
                    Some(msg) = rx.recv() => {
                        let now = Instant::now();
                        match msg {
                            MqttMessage::MqttOut(_, instant) if message_expired(&now, &instant) => {
                                warn!("Discarding outgoing message as too old");
                            },
                            MqttMessage::MqttOut(msg, _) => cli.publish(msg).await.unwrap(),
                        }
                    }
                    else => { break; }
                };
            }
        });

        self.b = Some(b);
    }
}

fn message_expired(now: &Instant, sent: &Instant) -> bool {
    (*now - *sent) > Duration::from_secs(300)
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
    tx: broadcast::Sender<String>,
}

pub struct Subscriptions(HashMap<String, Subscription>);

impl Subscriptions {
    pub fn new() -> Self {
        Subscriptions(HashMap::new())
    }

    fn get(&self, topic: &str) -> Option<&Subscription> {
        self.0.get(topic)
    }

    pub fn subscribe(&mut self, topic: &str) -> Pipe<String> {
        // Per subscription incoming MQTT queue.
        if let Some(subscription) = self.0.get(topic) {
            Pipe((), subscription.tx.clone())
        } else {
            let output = Pipe::new();

            let subscription = Subscription {
                topic: topic.to_string(),
                tx: output.get_tx(),
            };

            self.0.insert(topic.to_string(), subscription);
            output
        }
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

pub fn publish(mut input: broadcast::Receiver<Message>, mqtt_out: &MqttOut) {
    let mqtt_out = (*mqtt_out).clone();
    spawn(async move {
        while let Ok(v) = input.recv().await {
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
                let now = Instant::now();
                // FIXME: add timeout?
                mqtt_out.0.try_send(MqttMessage::MqttOut(v, now)).unwrap();
            }
        }
    });
}
