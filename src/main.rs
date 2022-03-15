use log::*;
use mqtt::Message;
use paho_mqtt as mqtt;
use std::{process, str, sync::mpsc, thread, time::Duration, collections::HashMap};

fn has_changed<T: Send + Eq + Clone + 'static>(input: mpsc::Receiver<T>) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut old_value: Option<T> = None;
        for v in input.iter() {
            let has_changed = if let Some(prev) = &old_value {
                *prev != v
            } else {
                true
            };
            if has_changed {
                old_value = Some(v.clone());
                tx.send(v).unwrap_or_else(|err| {
                    error!("send operation failed {err}");
                });
            }
        }
    });

    rx
}

fn publish(input: mpsc::Receiver<String>, mqtt_out: mpsc::Sender<MainMessage>) {
    thread::spawn(move || {
        for v in input {
            let msg = Message::new("test", v, 0);
            mqtt_out.send(MainMessage::MqttOut(msg)).unwrap();
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
    let topics: Vec<_> = subscriptions.0.iter().map(|(topic, _)| topic.clone()).collect();
    let qos: Vec<_> = subscriptions.0.iter().map(|_| 0).collect();

    if let Err(e) = cli.subscribe_many(&topics, &qos) {
        error!("Error subscribes topics: {:?}", e);
    }
}

enum MainMessage {
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
        let (tx, rx) = mpsc::channel();
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

fn main() {
    // Define the set of options for the create.
    // Use an ID for a persistent session.
    let create_opts = mqtt::CreateOptionsBuilder::new()
        .server_uri("tcp://broker.emqx.io:1883")
        .client_id("rust-nodes".to_string())
        .finalize();

    // Create a client.
    let mut cli = mqtt::Client::new(create_opts).unwrap_or_else(|err| {
        error!("Error creating the client: {:?}", err);
        process::exit(1);
    });

    // Define the set of options for the connection.
    let conn_opts = mqtt::ConnectOptionsBuilder::new()
        .keep_alive_interval(Duration::from_secs(20))
        .clean_session(true)
        .finalize();

    // Connect and wait for it to complete or fail.
    if let Err(e) = cli.connect(conn_opts) {
        error!("Unable to connect:\n\t{:?}", e);
        process::exit(1);
    }

    let (main_tx, main_rx) = mpsc::channel();
    let mqtt_in_rx = cli.start_consuming();

    let subscriptions: Subscriptions = setup_pipes(&main_tx);

    let a = thread::spawn(move || {
        for msg_or_none in mqtt_in_rx.iter() {
            main_tx.send(MainMessage::MqttIn(msg_or_none)).unwrap();
        }
    });

    let b = thread::spawn(move || {
        // Subscribe topics.
        subscribe_topics(&cli, &subscriptions);

        for msg in main_rx {
            let msg: MainMessage = msg;
            match msg {
                MainMessage::MqttIn(msg_or_none) => {
                    if let Some(msg) = msg_or_none {
                        let topic = msg.topic();
                        let payload = msg.payload();
                        let payload = str::from_utf8(payload).unwrap().to_string();
                        for subscription in subscriptions.get(topic) {
                                subscription.tx.send(payload.clone()).unwrap();
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
                MainMessage::MqttOut(msg) => cli.publish(msg).unwrap(),
            }
        }
    });

    a.join().unwrap();
    b.join().unwrap();
}

fn setup_pipes(main_tx: &mpsc::Sender<MainMessage>) -> Subscriptions {
    let mut subscriptions: Subscriptions = Subscriptions::new();

    let rx = subscriptions.subscription("ssss");
    let rx = has_changed(rx);
    publish(rx, main_tx.clone());

    subscriptions
}
