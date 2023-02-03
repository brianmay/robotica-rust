//! Source (and sink) for MQTT data.
pub mod topics;

use rumqttc::tokio_rustls::rustls::ClientConfig;
use rumqttc::v5::mqttbytes::v5::Packet;
use rumqttc::v5::mqttbytes::{Filter, Publish, RetainForwardRule};
use rumqttc::v5::{AsyncClient, ClientError, Event, Incoming, MqttOptions};
use rumqttc::{Outgoing, Transport};
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

use crate::entities::{self, Receiver, StatefulData};
use crate::{get_env, is_debug_mode, spawn, EnvironmentError};

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
    let payload = str::from_utf8(&msg.payload)?.to_string();
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
        oneshot::Sender<Result<Receiver<MqttMessage>, SubscribeError>>,
    ),
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
    ) -> Result<Receiver<MqttMessage>, SubscribeError> {
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
    pub async fn subscribe_into_stateless<T>(
        &self,
        topic: impl Into<String> + Send,
    ) -> Result<Receiver<T>, SubscribeError>
    where
        T: TryFrom<MqttMessage> + Clone + Send + 'static,
        <T as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
    {
        Ok(self.subscribe(topic).await?.translate_into_stateless::<T>())
    }

    /// Add new subscription and parse incoming data as type T
    ///
    /// # Errors
    ///
    /// Returns an error if the subscribe request could not be sent.
    pub async fn subscribe_into_stateful<T>(
        &self,
        topic: impl Into<String> + Send,
    ) -> Result<Receiver<StatefulData<T>>, SubscribeError>
    where
        T: TryFrom<MqttMessage> + Clone + Eq + Send + 'static,
        <T as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
    {
        Ok(self.subscribe(topic).await?.translate_into_stateful::<T>())
    }
}

/// An error loading the Config.
#[derive(Error, Debug)]
pub enum MqttClientError {
    /// Environment variable not set.
    #[error("Environment variable missing: {0}")]
    VarError(#[from] EnvironmentError),

    /// Environment variable set but invalid.
    #[error("Environment variable {0} invalid {1}")]
    VarInvalid(String, String, ParseIntError),
}

/// Client struct used to connect to MQTT.
pub struct MqttRx(mpsc::Receiver<MqttCommand>);

/// Create a new MQTT client.
#[must_use]
pub fn mqtt_channel() -> (MqttTx, MqttRx) {
    // Outgoing MQTT queue.
    let (tx, rx) = mpsc::channel(50);
    (MqttTx(tx), MqttRx(rx))
}

/// Connect to the MQTT broker and send/receive messages.
///
/// # Errors
///
/// Returns an error if there is a problem with the configuration.
pub fn run_client(
    mut subscriptions: Subscriptions,
    channel: MqttRx,
) -> Result<(), MqttClientError> {
    let mqtt_host = get_env("MQTT_HOST")?;
    let mqtt_port = get_env("MQTT_PORT")?;
    let mqtt_port = mqtt_port
        .parse()
        .map_err(|e| MqttClientError::VarInvalid("MQTT_PORT".to_string(), mqtt_port, e))?;
    let username = get_env("MQTT_USERNAME")?;
    let password = get_env("MQTT_PASSWORD")?;
    // let trust_store = env::var("MQTT_CA_CERT_FILE").unwrap();

    let hostname = gethostname::gethostname();
    let hostname = hostname.to_str().unwrap_or("unknown");
    let client_id = format!("robotica-rust-{hostname}");

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
    mqtt_options.set_max_packet_size(100 * 1024, 100 * 10 * 1024);
    // mqtt_options.set_clean_session(false);

    let (client, mut event_loop) = AsyncClient::new(mqtt_options, 50);

    // let trust_store = env::var("MQTT_CA_CERT_FILE").unwrap();

    spawn(async move {
        let mut rx = channel.0;

        loop {
            select! {
                event = event_loop.poll() => {
                    match event {
                        Ok(Event::Incoming(i)) => {
                            incoming_event(&client, *i, &subscriptions);
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
                            let debug_mode: bool = is_debug_mode();

                            debug!(
                                "Outgoing mqtt {} {} {}.",
                                if debug_mode { "nop" } else { "live" },
                                msg.retain,
                                msg.topic
                            );

                            if let Some(subscription) = subscriptions.get(&msg.topic) {
                                debug!("Looping message: {:?}", msg);
                                subscription.tx.try_send(msg.clone());
                            }

                            if !debug_mode {
                                if let Err(err) = client.try_publish(msg.topic, qos_to_rumqttc(msg.qos), msg.retain, msg.payload) {
                                    error!("Failed to publish message: {:?}.", err);
                                }
                            }
                        },
                        MqttCommand::Subscribe(topic, tx) => {
                            process_subscribe(&client, &mut subscriptions, &topic, tx);
                        }
                    }
                }
                else => { break; }
            };
        }
    });
    Ok(())
}

#[allow(clippy::cognitive_complexity)]
fn process_subscribe(
    client: &AsyncClient,
    subscriptions: &mut Subscriptions,
    topic: &str,
    tx: oneshot::Sender<Result<Receiver<MqttMessage>, SubscribeError>>,
) {
    debug!("Subscribing to topic: {}.", topic);
    let response = if let Some(subscription) = subscriptions.0.get(topic) {
        Ok(subscription.rx.clone())
    } else {
        let (tx, rx) = entities::create_stateless_entity(topic);

        let subscription = Subscription {
            topic: topic.to_string(),
            tx,
            rx: rx.clone(),
        };

        let filter = topic_to_filter(topic);
        match client.try_subscribe_many([filter]) {
            Ok(_) => {
                debug!("Subscribed to topic: {:?}.", topic);
                subscriptions.0.insert(topic.to_string(), subscription);
                Ok(rx)
            }
            Err(err) => {
                error!("Failed to subscribe to topics: {:?}.", err);
                Err(err.into())
            }
        }
    };

    if let Err(err) = tx.send(response) {
        error!("Failed to send subscribe response: {:?}.", err);
    }
}

fn incoming_event(client: &AsyncClient, pkt: Packet, subscriptions: &Subscriptions) {
    match pkt {
        Incoming::Publish(p, _) => match publish_to_mqtt_message(&p) {
            Ok(msg) => {
                let msg: MqttMessage = msg;
                let topic = &msg.topic;
                debug!("Received message: {msg:?}.");
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
    tx: entities::Sender<MqttMessage>,
    rx: entities::Receiver<MqttMessage>,
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
    pub fn subscribe(&mut self, topic: impl Into<String>) -> entities::Receiver<MqttMessage> {
        // Per subscription incoming MQTT queue.
        let topic = topic.into();
        if let Some(subscription) = self.0.get(&topic) {
            subscription.rx.clone()
        } else {
            let (tx, rx) = entities::create_stateless_entity(topic.clone());

            let subscription = Subscription {
                topic: topic.clone(),
                tx,
                rx: rx.clone(),
            };

            self.0.insert(topic, subscription);
            rx
        }
    }

    /// Add new subscription and parse incoming data as type T
    pub fn subscribe_into_stateless<T>(&mut self, topic: impl Into<String>) -> entities::Receiver<T>
    where
        T: TryFrom<MqttMessage> + Clone + Send + 'static,
        <T as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
    {
        self.subscribe(topic).translate_into_stateless::<T>()
    }

    /// Add new subscription and parse incoming data as type T
    pub fn subscribe_into_stateful<T>(
        &mut self,
        topic: impl Into<String>,
    ) -> entities::Receiver<StatefulData<T>>
    where
        T: TryFrom<MqttMessage> + Clone + Eq + Send + 'static,
        <T as TryFrom<MqttMessage>>::Error: Send + std::error::Error,
    {
        self.subscribe(topic).translate_into_stateful::<T>()
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
        retain_forward_rule: RetainForwardRule::OnEverySubscribe,
        ..Default::default()
    }
}

fn subscribe_topics(client: &AsyncClient, subscriptions: &Subscriptions) {
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

        let data: String = msg.payload;
        assert_eq!(data, "test");
    }

    #[test]
    fn test_string_to_message() {
        let msg = MqttMessage::new("test", "test".to_string(), false, QoS::AtLeastOnce);
        assert_eq!(msg.topic, "test");
        assert_eq!(msg.payload, "test");
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
}
