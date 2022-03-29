use anyhow::Result;
use log::*;
use paho_mqtt as mqtt;
use paho_mqtt::Message;
use std::cmp::min;
use std::collections::HashMap;
use std::time::Duration;
use std::{env, str, thread};
use tokio::sync::mpsc;

use crate::spawn;

#[derive(Debug)]
pub enum MqttMessage {
    MqttIn(Option<Message>),
    MqttOut(Message),
}

pub struct Mqtt {
    a: Option<thread::JoinHandle<()>>,
    b: Option<thread::JoinHandle<()>>,
    rx: Option<mpsc::Receiver<MqttMessage>>,
    tx: Option<mpsc::Sender<MqttMessage>>,
    tx_private: Option<mpsc::Sender<MqttMessage>>,
}

impl Mqtt {
    pub async fn new() -> Self {
        let (main_tx, main_rx) = mpsc::channel(10);

        Mqtt {
            a: None,
            b: None,
            rx: Some(main_rx),
            tx: Some(main_tx.clone()),
            tx_private: Some(main_tx),
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

        let create_opts = mqtt::CreateOptionsBuilder::new()
            .server_uri(&uri)
            .client_id(client_id) // FIXME: This is bad
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
                .user_name(env::var("MQTT_USERNAME").expect("MQTT_USERNAME should be set"))
                .password(env::var("MQTT_PASSWORD").expect("MQTT_PASSWORD should be set"))
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
                            debug!("incoming mqtt {topic} {payload}");
                            for subscription in subscriptions.get(topic) {
                                subscription.tx.blocking_send(payload.clone()).unwrap();
                            }
                        } else if !cli.is_connected() {
                            try_reconnect(&cli);
                            debug!("Resubscribe topics...");
                            subscribe_topics(&cli, &subscriptions);
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

impl Default for Subscriptions {
    fn default() -> Self {
        Self::new()
    }
}

fn try_reconnect(cli: &mqtt::Client) {
    let mut attempt: u32 = 1;
    loop {
        let sleep_time = 1000 * (attempt as u64).checked_pow(2).unwrap();
        let sleep_time = min(60_000, sleep_time);

        warn!("Connection lost to mqtt. Waiting {sleep_time} ms to retry connection attempt {attempt}.");
        thread::sleep(Duration::from_millis(sleep_time));

        warn!("Trying to connect to mqtt");
        if cli.reconnect().is_ok() {
            warn!("Successfully reconnected to mqtt");
            break;
        }

        attempt = attempt.saturating_add(1);
    }
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
                mqtt_out.send(MqttMessage::MqttOut(v)).await.unwrap();
            }
        }
    });
}
