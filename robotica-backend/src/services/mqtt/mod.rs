//! Source (and sink) for MQTT data.
pub mod topics;

use rumqttc::tokio_rustls::rustls::{self, ClientConfig, RootCertStore};
use rumqttc::v5::mqttbytes::v5::{Filter, Packet, Publish};
use rumqttc::v5::{AsyncClient, ClientError, Event, Incoming, MqttOptions};
use rumqttc::{Outgoing, Transport};
use serde::Deserialize;
use std::collections::HashMap;
use std::num::ParseIntError;
use std::str;
use std::str::Utf8Error;
use thiserror::Error;
use tokio::select;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Duration};
use tracing::{debug, error};

use robotica_common::mqtt::{MqttMessage, QoS};

use crate::pipes::{generic, stateful, stateless};
use crate::spawn;

const NUMBER_OF_STARTUP_MESSAGES: usize = 100;
const NUMBER_OF_STARTUP_SUBSCRIPTIONS: usize = 100;

const fn qos_to_rumqttc(qos: QoS) -> rumqttc::v5::mqttbytes::QoS {
    match qos {
        QoS::AtMostOnce => rumqttc::v5::mqttbytes::QoS::AtMostOnce,
        QoS::AtLeastOnce => rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
        QoS::ExactlyOnce => rumqttc::v5::mqttbytes::QoS::ExactlyOnce,
    }
}

const fn qos_from_rumqttc(qos: rumqttc::v5::mqttbytes::QoS) -> QoS {
    match qos {
        rumqttc::v5::mqttbytes::QoS::AtMostOnce => QoS::AtMostOnce,
        rumqttc::v5::mqttbytes::QoS::AtLeastOnce => QoS::AtLeastOnce,
        rumqttc::v5::mqttbytes::QoS::ExactlyOnce => QoS::ExactlyOnce,
    }
}

fn publish_to_mqtt_message(msg: &Publish) -> Result<MqttMessage, Utf8Error> {
    let topic = str::from_utf8(&msg.topic)?.to_string();
    let payload = msg.payload.to_vec();
    Ok(MqttMessage {
        topic,
        payload,
        retain: msg.retain,
        qos: qos_from_rumqttc(msg.qos),
    })
}

/// An error occurred during a `Mqtt` subscribe operation.
#[derive(Error, Debug)]
pub enum SubscribeError {
    /// Send error
    #[error("Send error")]
    SendError(),

    /// Receive error
    #[error("Receive error: {0}")]
    ReceiveError(#[from] oneshot::error::RecvError),

    /// Client error
    #[error("Client error: {0}")]
    ClientError(#[from] ClientError),
}

#[derive(Debug)]
enum MqttCommand {
    MqttOut(MqttMessage),
    Subscribe(
        String,
        oneshot::Sender<Result<generic::Receiver<MqttMessage>, SubscribeError>>,
    ),
    Unsubscribe(String),
}

/// Struct used to send outgoing MQTT messages.
#[derive(Clone)]
pub struct MqttTx(mpsc::Sender<MqttCommand>);

impl MqttTx {
    /// Send a message to the MQTT broker.
    pub fn try_send(&self, msg: MqttMessage) {
        let _ = self
            .0
            .try_send(MqttCommand::MqttOut(msg))
            .map_err(|e| error!("MQTT send error: {}", e));
    }

    /// Subscribe to a topic and return a receiver for the messages.
    /// The receiver will be closed when the MQTT connection is closed.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscribe request could not be sent.
    pub async fn subscribe(
        &self,
        topic: impl Into<String> + Send,
    ) -> Result<generic::Receiver<MqttMessage>, SubscribeError> {
        let (tx, rx) = oneshot::channel();
        self.0
            .send(MqttCommand::Subscribe(topic.into(), tx))
            .await
            .map_err(|_| SubscribeError::SendError())?;
        rx.await?
    }

    /// Add new subscription and parse incoming data as type T
    ///
    /// # Errors
    ///
    /// Returns an error if the subscribe request could not be sent.
    pub async fn subscribe_into_stateless<U>(
        &self,
        topic: impl Into<String> + Send,
    ) -> Result<stateless::Receiver<U>, SubscribeError>
    where
        // U: Data,
        // T::Sent: Send + 'static,
        U: TryFrom<MqttMessage> + Clone + Send + 'static,
        <U as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
        // T::Received: Send + 'static,
    {
        Ok(self
            .subscribe(topic)
            .await?
            .into_stateless()
            .translate::<U>())
    }

    /// Add new subscription and parse incoming data as type T
    ///
    /// # Errors
    ///
    /// Returns an error if the subscribe request could not be sent.
    pub async fn subscribe_into_stateful<U>(
        &self,
        topic: impl Into<String> + Send,
    ) -> Result<stateful::Receiver<U>, SubscribeError>
    where
        // U: Data,
        // T::Sent: Send + 'static,
        U: TryFrom<MqttMessage> + Clone + Send + Eq + 'static,
        <U as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
        // T::Received: Send + 'static,
    {
        Ok(self
            .subscribe(topic)
            .await?
            .into_stateful()
            .translate::<U>())
    }
}

/// An error loading the Config.
#[derive(Error, Debug)]
pub enum MqttClientError {
    /// Environment variable set but invalid.
    #[error("Environment variable {0} invalid {1}")]
    VarInvalid(String, String, ParseIntError),
}

/// Client struct used to connect to MQTT.
pub struct MqttRx {
    tx: mpsc::Sender<MqttCommand>,
    rx: mpsc::Receiver<MqttCommand>,
}

/// Create a new MQTT client.
#[must_use]
pub fn mqtt_channel() -> (MqttTx, MqttRx) {
    // Outgoing MQTT queue.
    let (tx, rx) = mpsc::channel(NUMBER_OF_STARTUP_MESSAGES);
    (MqttTx(tx.clone()), MqttRx { tx, rx })
}

/// Credentials for MQTT
#[derive(Deserialize, Default)]
#[serde(tag = "type")]
pub enum Credentials {
    /// Username and password
    UsernamePassword {
        /// Username
        username: String,

        /// Password
        password: String,
    },

    /// No credentials
    #[default]
    None,
}

#[derive(Deserialize)]
/// MQTT configuration
pub struct Config {
    /// MQTT host
    pub host: String,

    /// MQTT port
    pub port: u16,

    /// MQTT username
    #[serde(default)]
    pub credentials: Credentials,
}

/// Connect to the MQTT broker and send/receive messages.
///
/// # Errors
///
/// Returns an error if there is a problem with the configuration.
pub fn run_client(
    mut subscriptions: Subscriptions,
    channel: MqttRx,
    config: Config,
) -> Result<(), MqttClientError> {
    let hostname = gethostname::gethostname();
    let hostname = hostname.to_str().unwrap_or("unknown");
    let client_id = format!("robotica-rust-{hostname}");

    let root_store = get_root_store();
    let client_config = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let mut mqtt_options = MqttOptions::new(client_id, config.host, config.port);
    mqtt_options.set_keep_alive(Duration::from_secs(30));

    if config.port == 8883 {
        mqtt_options.set_transport(Transport::tls_with_config(client_config.into()));
    }

    match config.credentials {
        Credentials::UsernamePassword { username, password } => {
            mqtt_options.set_credentials(username, password);
        }
        Credentials::None => {}
    }
    mqtt_options.set_max_packet_size(Some(100 * 10 * 1024));
    // mqtt_options.set_clean_session(false);

    let (client, mut event_loop) = AsyncClient::new(mqtt_options, NUMBER_OF_STARTUP_SUBSCRIPTIONS);

    // let trust_store = env::var("MQTT_CA_CERT_FILE").unwrap();

    // error!("Number of subscriptions: {}", subscriptions.0.len());

    for subscription in subscriptions.iter() {
        watch_tx_closed(
            subscription.tx.clone(),
            channel.tx.clone(),
            subscription.topic.clone(),
        );
    }

    spawn(async move {
        let mut rx = channel.rx;

        loop {
            select! {
                event = event_loop.poll() => {
                    match event {
                        Ok(Event::Incoming(i)) => {
                            incoming_event(&client, i, &subscriptions);
                        },
                        Ok(Event::Outgoing(o)) => {
                            if let Outgoing::Publish(p) = o {
                                    debug!("Published message: {:?}.", p);
                            }
                        },
                        Err(err) => {
                            error!("MQTT Error: {:?}", err);
                            sleep(Duration::from_secs(10)).await;
                        }
                    }
                },
                Some(msg) = rx.recv() => {
                    match msg {
                        MqttCommand::MqttOut(msg) => {
                            debug!(
                                "Outgoing mqtt {} {}.",
                                msg.retain,
                                msg.topic
                            );

                            if let Some(subscription) = subscriptions.get(&msg.topic) {
                                debug!("Looping message: {:?}", msg);
                                subscription.tx.try_send(msg.clone());
                            }

                            if let Err(err) = client.try_publish(msg.topic, qos_to_rumqttc(msg.qos), msg.retain, msg.payload) {
                                error!("Failed to publish message: {:?}.", err);
                            }
                        },
                        MqttCommand::Subscribe(topic, tx) => {
                            process_subscribe(&client, &mut subscriptions, &topic, tx, channel.tx.clone());
                        }
                        MqttCommand::Unsubscribe(topic) => {
                            debug!("Unsubscribing from topic: {}.", topic);
                            if let Some(subscription) = subscriptions.unsubscribe(&topic) {
                                if let Err(err) = client.try_unsubscribe(&topic) {
                                    error!("Failed to unsubscribe from topic: {:?}.", err);
                                }
                                drop(subscription);
                            }
                        }
                    }
                }
                else => { break; }
            };
        }
    });
    Ok(())
}

fn get_root_store() -> RootCertStore {
    let mut root_store = rustls::RootCertStore::empty();

    let certs = match rustls_native_certs::load_native_certs() {
        Ok(certs) => certs,
        Err(err) => {
            error!("Failed to load native certs: {:?}", err);
            return root_store;
        }
    };

    for cert in certs {
        _ = root_store
            .add(&rustls::Certificate(cert.as_ref().to_vec()))
            .map_err(|err| {
                error!("Failed to add certificate: {:?}", err);
            });
    }

    root_store
}

#[allow(clippy::cognitive_complexity)]
fn process_subscribe(
    client: &AsyncClient,
    subscriptions: &mut Subscriptions,
    topic: impl Into<String>,
    tx: oneshot::Sender<Result<generic::Receiver<MqttMessage>, SubscribeError>>,
    channel_tx: mpsc::Sender<MqttCommand>,
) {
    let topic: String = topic.into();

    debug!("Subscribing to topic: {}.", topic);
    let subscription = subscriptions.0.get(&topic);
    let maybe_rx = subscription.and_then(|s| s.rx.upgrade());

    let response = if let Some(rx) = maybe_rx {
        Ok(rx)
    } else {
        let (tx, rx) = generic::create_pipe(&topic);

        let subscription = Subscription {
            topic: topic.to_string(),
            tx: tx.clone(),
            rx: rx.downgrade(),
        };

        let filter = topic_to_filter(&topic);
        match client.try_subscribe_many([filter]) {
            Ok(()) => {
                debug!("Subscribed to topic: {:?}.", topic);
                subscriptions.0.insert(topic.to_string(), subscription);
                watch_tx_closed(tx, channel_tx, topic);
                Ok(rx)
            }
            Err(err) => {
                error!("Failed to subscribe to topics: {:?}.", err);
                subscriptions.0.remove(&topic);
                Err(err.into())
            }
        }
    };

    if let Err(err) = tx.send(response) {
        error!("Failed to send subscribe response: {:?}.", err);
    }
}

fn watch_tx_closed(
    tx: generic::Sender<MqttMessage>,
    channel_tx: mpsc::Sender<MqttCommand>,
    topic: String,
) {
    spawn(async move {
        tx.closed().await;
        debug!("Entity for subscription closed: {:?}.", topic);
        channel_tx
            .send(MqttCommand::Unsubscribe(topic))
            .await
            .unwrap_or_else(|err| {
                error!("Failed to send unsubscribe command: {:?}.", err);
            });
    });
}

fn incoming_event(client: &AsyncClient, pkt: Packet, subscriptions: &Subscriptions) {
    match pkt {
        Incoming::Publish(p) => match publish_to_mqtt_message(&p) {
            Ok(msg) => {
                let msg: MqttMessage = msg;
                let topic = &msg.topic;
                // debug!("Received message: {msg:?}.");
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
    tx: generic::Sender<MqttMessage>,
    rx: generic::WeakReceiver<MqttMessage>,
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
    pub fn subscribe(&mut self, topic: impl Into<String>) -> generic::Receiver<MqttMessage> {
        // Per subscription incoming MQTT queue.
        let topic = topic.into();
        let subscription = self.0.get(&topic);
        let maybe_rx = subscription.and_then(|s| s.rx.upgrade());

        if let Some(rx) = maybe_rx {
            rx
        } else {
            let (tx, rx) = generic::create_pipe(topic.clone());

            let subscription = Subscription {
                topic: topic.clone(),
                tx,
                rx: rx.downgrade(),
            };

            self.0.insert(topic, subscription);
            rx
        }
    }

    /// Add new subscription and parse incoming data as type T
    pub fn subscribe_into_stateless<T>(
        &mut self,
        topic: impl Into<String>,
    ) -> stateless::Receiver<T>
    where
        T: TryFrom<MqttMessage> + Clone + Send + 'static,
        <T as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
    {
        self.subscribe(topic).into_stateless().translate()
    }

    /// Add new subscription and parse incoming data as type T
    pub fn subscribe_into_stateful<T>(&mut self, topic: impl Into<String>) -> stateful::Receiver<T>
    where
        T: TryFrom<MqttMessage> + Clone + Eq + Send + 'static,
        <T as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
    {
        self.subscribe(topic).into_stateful().translate()
    }

    /// Iterate over all subscriptions.
    fn iter(&self) -> impl Iterator<Item = &Subscription> {
        self.0.values()
    }

    /// Remove a subscription from the list.
    fn unsubscribe(&mut self, topic: &str) -> Option<Subscription> {
        self.0.remove(topic)
    }
}

impl Default for Subscriptions {
    fn default() -> Self {
        Self::new()
    }
}

fn topic_to_filter(topic: &str) -> Filter {
    Filter {
        path: topic.to_string(),
        qos: rumqttc::v5::mqttbytes::QoS::ExactlyOnce,
        nolocal: true,
        // retain_forward_rule: rumqttc::RetainForwardRule::Forward,
        ..Default::default()
    }
}

fn subscribe_topics(client: &AsyncClient, subscriptions: &Subscriptions) {
    if subscriptions.0.is_empty() {
        return;
    }

    let topics = subscriptions.0.keys().map(|topic| topic_to_filter(topic));

    if let Err(e) = client.try_subscribe_many(topics) {
        error!("Error subscribing to topics: {:?}", e);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_message_to_string() {
        let msg = MqttMessage {
            topic: "test".to_string(),
            payload: "test".into(),
            qos: QoS::AtLeastOnce,
            retain: false,
        };

        let data: String = msg.try_into().unwrap();
        assert_eq!(data, "test");
    }

    #[test]
    fn test_string_to_message() {
        let msg = MqttMessage::new("test", "test".to_string(), false, QoS::AtLeastOnce);
        assert_eq!(msg.topic, "test");
        assert_eq!(msg.payload, b"test");
        assert_eq!(msg.qos, QoS::AtLeastOnce);
        assert!(!msg.retain);
    }

    #[test]
    fn test_message_to_bool() {
        let msg = MqttMessage {
            topic: "test".to_string(),
            payload: "true".into(),
            qos: QoS::AtLeastOnce,
            retain: false,
        };

        let data: bool = msg.try_into().unwrap();
        assert!(data);
    }

    #[test]
    fn test_deserialize_username_password_config() {
        let config = r#"
            host: "test"
            port: 1234
            credentials:
                type: "UsernamePassword"
                username: "test"
                password: "test"
        "#;

        let config = serde_yaml::from_str::<Config>(config).unwrap();
        assert_eq!(config.host, "test");
        assert_eq!(config.port, 1234);
        if let Credentials::UsernamePassword { username, password } = config.credentials {
            assert_eq!(username, "test");
            assert_eq!(password, "test");
        } else {
            panic!("Invalid credentials");
        }
    }

    #[test]
    fn test_deserialize_anonymous_config() {
        let config = r#"
            host: "test"
            port: 1234
        "#;

        let config = serde_yaml::from_str::<Config>(config).unwrap();
        assert_eq!(config.host, "test");
        assert_eq!(config.port, 1234);
        assert!(matches!(config.credentials, Credentials::None));
    }
}
