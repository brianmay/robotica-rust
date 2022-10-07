//! Source (and sink) for MQTT data.
use bytes::Bytes;
use log::{debug, error, info, warn};
use rumqttc::tokio_rustls::rustls::ClientConfig;
use rumqttc::v5::mqttbytes::v5::Packet;
use rumqttc::v5::mqttbytes::{Filter, Publish};
use rumqttc::v5::{AsyncClient, Event, Incoming, MqttOptions};
use rumqttc::{Outgoing, Transport};
use std::collections::HashMap;
use std::str::Utf8Error;
use std::time::Duration;
use std::{env, str};
use thiserror::Error;
use tokio::select;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Instant};

use crate::entities;
// use crate::entities::FromTranslate;
use crate::spawn;

/// `QoS` for MQTT messages.
pub type QoS = rumqttc::v5::mqttbytes::QoS;

/// A received/sent MQTT message
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The topic of the message
    pub topic: String,

    /// The raw unparsed payload of the message
    pub payload: Bytes,

    /// Was/Is this message retained?
    pub retain: bool,

    /// What is the QoS of this message?
    qos: QoS,

    /// What was the instant this message was created?
    instant: Instant,
}

impl Message {
    /// Create a new message.
    pub fn new(topic: &str, payload: Bytes, retain: bool, qos: QoS) -> Self {
        Self {
            topic: topic.to_string(),
            payload,
            retain,
            qos,
            instant: Instant::now(),
        }
    }

    /// Create a message from a string.
    #[must_use]
    pub fn from_string(topic: &str, payload: &str, retain: bool, qos: QoS) -> Message {
        Message {
            topic: topic.to_string(),
            payload: payload.to_string().into(),
            retain,
            qos,
            instant: Instant::now(),
        }
    }
}

impl TryFrom<Publish> for Message {
    type Error = Utf8Error;

    fn try_from(msg: Publish) -> Result<Self, Self::Error> {
        let topic = str::from_utf8(&msg.topic)?.to_string();
        Ok(Self {
            topic,
            payload: msg.payload,
            retain: msg.retain,
            qos: msg.qos,
            instant: Instant::now(),
        })
    }
}

impl TryFrom<Message> for String {
    type Error = Utf8Error;

    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        Ok(str::from_utf8(&msg.payload)?.to_string())
    }
}

/// An error when translating a message into a boolean.
#[derive(Error, Debug)]
pub enum BoolError {
    /// The payload was not a valid boolean string.
    #[error("Invalid value: {0}")]
    InvalidValue(String),

    /// The payload was not valid UTF8.
    #[error("Invalid UTF8")]
    Utf8Error(#[from] std::str::Utf8Error),
}

impl TryFrom<Message> for bool {
    type Error = BoolError;

    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        let payload: String = msg.try_into()?;
        match payload.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            value => Err(BoolError::InvalidValue(value.to_string())),
        }
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
    #[must_use]
    pub fn new() -> Self {
        // Outgoing MQTT queue.
        let (tx, rx) = mpsc::channel(50);

        MqttClient {
            b: None,
            tx: Some(tx),
            rx: Some(rx),
        }
    }

    /// Get the `MqttOut` struct for sending outgoing messages.
    ///
    /// # Panics
    ///
    /// Panics if the this has already been called since the last call to `new`.
    pub fn get_mqtt_out(&mut self) -> MqttOut {
        MqttOut(self.tx.take().unwrap())
    }

    /// Connect to the MQTT broker.
    ///
    /// # Panics
    ///
    /// Panics if the environment variables are not set.
    ///
    /// FIXME: We should return error without panic.
    pub fn connect(&mut self, subscriptions: Subscriptions) {
        let mqtt_host = env::var("MQTT_HOST").expect("MQTT_HOST should be set");
        let mqtt_port = env::var("MQTT_PORT")
            .expect("MQTT_PORT should be set")
            .parse()
            .unwrap();
        let username = env::var("MQTT_USERNAME").expect("MQTT_USERNAME should be set");
        let password = env::var("MQTT_PASSWORD").expect("MQTT_PASSWORD should be set");
        // let trust_store = env::var("MQTT_CA_CERT_FILE").unwrap();

        let hostname = gethostname::gethostname();
        let hostname = hostname.to_str().unwrap();
        let client_id = format!("robotica-node-rust-{hostname}");

        let mut root_store = rustls::RootCertStore::empty();
        root_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|ta| {
            rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
                ta.subject,
                ta.spki,
                ta.name_constraints,
            )
        }));
        let client_config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let mut mqtt_options = MqttOptions::new(client_id, mqtt_host, mqtt_port);
        mqtt_options.set_keep_alive(Duration::from_secs(30));
        mqtt_options.set_transport(Transport::tls_with_config(client_config.into()));
        mqtt_options.set_credentials(username, password);
        // mqtt_options.set_clean_session(false);

        let rx = self.rx.take().unwrap();
        let b = spawn(async move {
            let (client, mut event_loop) = AsyncClient::new(mqtt_options, 10);

            // let trust_store = env::var("MQTT_CA_CERT_FILE").unwrap();

            let mut rx = rx;

            // Subscribe topics.
            subscribe_topics(&client, &subscriptions);

            loop {
                select! {
                    event = event_loop.poll() => {
                        match event {
                            Ok(Event::Incoming(i)) => {
                                incoming_event(&client, *i, &subscriptions);
                            },
                            Ok(Event::Outgoing(o)) => {
                                if let Outgoing::Publish(p) = o {
                                        println!("Published message: {:?}.", p);
                                }
                            },
                            Err(err) => {
                                error!("MQTT Error: {:?}", err);
                                sleep(Duration::from_secs(10)).await;
                            }
                        }
                    },
                    Some(msg) = rx.recv() => {
                        let now = Instant::now();
                        match msg {
                            MqttMessage::MqttOut(msg) if message_expired(&now, &msg.instant) => {
                                warn!("Discarding outgoing message as too old.");
                            },
                            MqttMessage::MqttOut(msg) => {
                                let debug_mode: bool = is_debug_mode();

                                info!(
                                    "outgoing mqtt {} {} {}.",
                                    if debug_mode { "nop" } else { "live" },
                                    msg.retain,
                                    msg.topic
                                );

                                if !debug_mode {
                                    if let Err(err) = client.try_publish(msg.topic, msg.qos, msg.retain, msg.payload) {
                                        error!("Failed to publish message: {:?}.", err);
                                    }
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
    ///
    /// # Panics
    ///
    /// Panics if the server exits with an error.
    pub async fn wait(&mut self) {
        if let Some(b) = self.b.take() {
            b.await.unwrap();
        }
    }
}

impl Default for MqttClient {
    fn default() -> Self {
        Self::new()
    }
}

fn message_expired(now: &Instant, sent: &Instant) -> bool {
    (*now - *sent) > Duration::from_secs(300)
}

fn incoming_event(client: &AsyncClient, pkt: Packet, subscriptions: &Subscriptions) {
    match pkt {
        Incoming::Publish(p, _) => match p.try_into() {
            Ok(msg) => {
                let msg: Message = msg;
                let topic = &msg.topic;
                debug!("Incoming mqtt {topic}.");
                if let Some(subscription) = subscriptions.get(topic) {
                    subscription.tx.try_send(msg);
                }
            }
            Err(err) => error!("Invalid message received: {err}"),
        },
        Incoming::ConnAck(_) => {
            debug!("Resubscribe topics.");
            subscribe_topics(client, subscriptions);
        }
        _ => {}
    }
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
    #[must_use]
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

fn subscribe_topics(client: &AsyncClient, subscriptions: &Subscriptions) {
    let topics = subscriptions.0.iter().map(|(topic, _)| Filter {
        path: topic.clone(),
        qos: QoS::ExactlyOnce,
        ..Default::default()
    });

    if let Err(e) = client.try_subscribe_many(topics) {
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
            payload: "test".into(),
            qos: QoS::AtLeastOnce,
            retain: false,
            instant: Instant::now(),
        };

        let data: String = msg.try_into().unwrap();
        assert_eq!(data, "test");
    }

    #[tokio::test]
    async fn test_string_to_message() {
        let msg = Message::from_string("test", "test", false, QoS::AtLeastOnce);
        assert_eq!(msg.topic, "test");
        assert_eq!(msg.payload, b"test"[..]);
        assert_eq!(msg.qos, QoS::AtLeastOnce);
        assert!(!msg.retain);
    }

    #[tokio::test]
    async fn test_message_to_bool() {
        let msg = Message {
            topic: "test".to_string(),
            payload: "true".into(),
            qos: QoS::AtLeastOnce,
            retain: false,
            instant: Instant::now(),
        };

        let data: bool = msg.try_into().unwrap();
        assert!(data);
    }
}
