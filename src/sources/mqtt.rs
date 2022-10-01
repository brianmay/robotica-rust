//! Source (and sink) for MQTT data.
use log::*;
use paho_mqtt::AsyncClient;
use paho_mqtt::ConnectOptionsBuilder;
use paho_mqtt::CreateOptionsBuilder;
use paho_mqtt::SslOptionsBuilder;
use std::cmp::min;
use std::collections::HashMap;
use std::str::Utf8Error;
use std::time::Duration;
use std::{env, str};
use tokio::select;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio::time::timeout;
use tokio::time::Instant;

use crate::entities;
// use crate::entities::FromTranslate;
use crate::spawn;

/// A received/sent MQTT message
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The topic of the message
    pub topic: String,

    /// The raw unparsed payload of the message
    pub payload: Vec<u8>,

    /// Was/Is this message retained?
    pub retain: bool,

    /// What is the QoS of this message?
    qos: i32,

    /// What was the instant this message was created?
    instant: Instant,
}

impl Message {
    /// Create a new message.
    pub fn new(topic: &str, payload: Vec<u8>, retain: bool, qos: i32) -> Self {
        Self {
            topic: topic.to_string(),
            payload,
            retain,
            qos,
            instant: Instant::now(),
        }
    }

    /// Create a message from a string.
    pub fn from_string(topic: &str, payload: &str, retain: bool, qos: i32) -> Message {
        Message {
            topic: topic.to_string(),
            payload: payload.as_bytes().to_vec(),
            retain,
            qos,
            instant: Instant::now(),
        }
    }
}

impl From<paho_mqtt::Message> for Message {
    fn from(msg: paho_mqtt::Message) -> Self {
        let topic = msg.topic().to_string();
        Self {
            topic,
            payload: msg.payload().to_vec(),
            retain: msg.retained(),
            qos: msg.qos(),
            instant: Instant::now(),
        }
    }
}

impl From<Message> for paho_mqtt::Message {
    fn from(msg: Message) -> Self {
        if msg.retain {
            Self::new_retained(msg.topic, msg.payload, msg.qos)
        } else {
            Self::new(msg.topic, msg.payload, msg.qos)
        }
    }
}

impl TryFrom<Message> for String {
    type Error = Utf8Error;

    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        Ok(str::from_utf8(&msg.payload)?.to_string())
    }
}

#[derive(Debug, Clone)]
enum MqttMessage {
    MqttOut(Message),
}

/// Client struct used to connect to MQTT.
pub struct MqttClient {
    b: Option<JoinHandle<()>>,
    tx: Option<mpsc::Sender<MqttMessage>>,
    rx: Option<mpsc::Receiver<MqttMessage>>,
}

/// Struct used to send outgoing MQTT messages.
#[derive(Clone)]
pub struct MqttOut(mpsc::Sender<MqttMessage>);

impl MqttOut {
    /// Send a message to the MQTT broker.
    pub async fn send(&self, msg: Message) {
        self.0
            .send(MqttMessage::MqttOut(msg))
            .await
            .expect("Failed to send message");
    }
}

impl MqttClient {
    /// Create a new MQTT client.
    pub async fn new() -> Self {
        // Outgoing MQTT queue.
        let (tx, rx) = mpsc::channel(50);

        MqttClient {
            b: None,
            tx: Some(tx),
            rx: Some(rx),
        }
    }

    /// Get the [MqttOut] struct for sending outgoing messages.
    pub fn get_mqtt_out(&mut self) -> MqttOut {
        MqttOut(self.tx.take().unwrap())
    }

    /// Connect to the MQTT broker.
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
                            let msg: Message = msg.into();
                            let topic = &msg.topic;
                            // let payload = msg.payload;
                            debug!("incoming mqtt {topic}");
                            if let Some(subscription) = subscriptions.get(topic) {
                                subscription.tx.send(msg).await;
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
                            MqttMessage::MqttOut(msg) if message_expired(&now, &msg.instant) => {
                                warn!("Discarding outgoing message as too old");
                            },
                            MqttMessage::MqttOut(msg) => {
                                let debug_mode: bool = is_debug_mode();

                                info!(
                                    "outgoing mqtt {} {} {}",
                                    if debug_mode { "nop" } else { "live" },
                                    msg.retain,
                                    msg.topic
                                );

                                if !debug_mode {
                                    cli.publish(msg.into
                                        ()).await.unwrap()
                                }
                            },
                        }
                    }
                    else => { break; }
                };
            }
        });

        self.b = Some(b);
    }

    /// Wait for the client to finish.
    pub async fn wait(&mut self) {
        if let Some(b) = self.b.take() {
            b.await.unwrap();
        }
    }
}

fn message_expired(now: &Instant, sent: &Instant) -> bool {
    (*now - *sent) > Duration::from_secs(300)
}

struct Subscription {
    #[allow(dead_code)]
    topic: String,
    tx: entities::Sender<Message>,
    rx: entities::Receiver<Message>,
}

/// List of all required subscriptions.
pub struct Subscriptions(HashMap<String, Subscription>);

impl Subscriptions {
    /// Create a new set of subscriptions.
    pub fn new() -> Self {
        Subscriptions(HashMap::new())
    }

    fn get(&self, topic: &str) -> Option<&Subscription> {
        self.0.get(topic)
    }

    /// Add a new subscription.
    pub fn subscribe(&mut self, topic: &str) -> entities::Receiver<Message> {
        // Per subscription incoming MQTT queue.
        if let Some(subscription) = self.0.get(topic) {
            subscription.rx.clone()
        } else {
            let (tx, rx) = entities::create_entity(topic);

            let subscription = Subscription {
                topic: topic.to_string(),
                tx,
                rx: rx.clone(),
            };

            self.0.insert(topic.to_string(), subscription);
            rx
        }
    }

    /// Add new subscription and parse incoming data as type T
    pub fn subscribe_into<T>(&mut self, topic: &str) -> entities::Receiver<T>
    where
        T: TryFrom<Message> + Clone + Eq + Send + 'static,
        <T as TryFrom<Message>>::Error: Send + std::error::Error,
    {
        self.subscribe(topic).translate_into::<T>()
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
        error!("Error subscribing to topics: {:?}", e);
    }
}

fn is_debug_mode() -> bool {
    match env::var("DEBUG_MODE") {
        Ok(value) => value.to_lowercase() == "true",
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_message_to_string() {
        let msg = Message {
            topic: "test".to_string(),
            payload: "test".as_bytes().to_vec(),
            qos: 0,
            retain: false,
            instant: Instant::now(),
        };

        let data: String = msg.try_into().unwrap();
        assert_eq!(data, "test");
    }

    #[tokio::test]
    async fn test_string_to_message() {
        let msg = Message::from_string("test", "test", false, 0);
        assert_eq!(msg.topic, "test");
        assert_eq!(msg.payload, "test".as_bytes());
        assert_eq!(msg.qos, 0);
        assert!(!msg.retain);
    }
}
