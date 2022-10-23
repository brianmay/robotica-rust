use std::collections::HashMap;

use futures::{
    channel::mpsc::Sender,
    future::{select, Either},
    SinkExt, StreamExt,
};
use gloo_timers::callback::Timeout;
use log::{debug, error, info};
use reqwasm::websocket::{futures::WebSocket, Message};
use wasm_bindgen_futures::spawn_local;
use web_sys::window;
use yew::Callback;

use super::robotica::{MqttMessage, WsCommand};

#[derive(Debug)]
pub enum Command {
    Subscribe {
        topic: String,
        callback: Callback<MqttMessage>,
    },
    EventHandler(Callback<WsEvent>),
    Send(MqttMessage),
    KeepAlive,
}

pub enum WsEvent {
    Disconnect,
    Connect,
}

#[derive(Clone)]
pub struct WebsocketService {
    pub tx: Sender<Command>,
}

impl PartialEq for WebsocketService {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

fn message_to_string(msg: Message) -> Option<String> {
    match msg {
        Message::Text(s) => Some(s),
        Message::Bytes(b) => match String::from_utf8(b) {
            Ok(s) => Some(s),
            Err(err) => {
                error!("Failed to convert binary message to string: {:?}", err);
                None
            }
        },
    }
}

impl WebsocketService {
    pub fn new() -> Self {
        let url = get_websocket_url();
        info!("Connecting to {}", url);

        let keep_alive_duration_millis = 15_000;
        let reconnect_delay_millis = 5000;

        let mut ws = WebSocket::open(&url).unwrap();
        let mut subscriptions: HashMap<String, Vec<Callback<MqttMessage>>> = HashMap::new();
        let mut event_handlers: Vec<Callback<WsEvent>> = vec![];
        let mut is_connected = true;
        let (in_tx, mut in_rx) = futures::channel::mpsc::channel::<Command>(10);
        let mut timeout = schedule_keep_alive(&in_tx, keep_alive_duration_millis);

        let in_tx_clone = in_tx.clone();
        spawn_local(async move {
            loop {
                match select(in_rx.next(), ws.next()).await {
                    Either::Left((Some(Command::Subscribe { topic, callback }), _)) => {
                        debug!("ws: Subscribing to {}", topic);
                        match subscriptions.get_mut(&topic) {
                            Some(callbacks) => callbacks.push(callback),
                            None => {
                                subscriptions.insert(topic.clone(), vec![callback]);
                                let command = WsCommand::Subscribe { topic };
                                ws.send(Message::Text(serde_json::to_string(&command).unwrap()))
                                    .await
                                    .unwrap();
                                timeout =
                                    schedule_keep_alive(&in_tx_clone, keep_alive_duration_millis);
                            }
                        }
                    }
                    Either::Left((Some(Command::Send(msg)), _)) => {
                        debug!("ws: Sending message: {:?}", msg);
                        let command = WsCommand::Send(msg);
                        ws.send(Message::Text(serde_json::to_string(&command).unwrap()))
                            .await
                            .unwrap();
                        timeout = schedule_keep_alive(&in_tx_clone, keep_alive_duration_millis);
                    }
                    Either::Left((Some(Command::EventHandler(handler)), _)) => {
                        debug!("ws: Register event handler.");
                        if is_connected {
                            handler.emit(WsEvent::Connect);
                        } else {
                            handler.emit(WsEvent::Disconnect);
                        }
                        event_handlers.push(handler);
                    }
                    Either::Left((Some(Command::KeepAlive), _)) => {
                        info!("ws: Got KeepAlive command.");
                        timeout.map(|t| t.forget());

                        if !is_connected {
                            for handler in &event_handlers {
                                handler.emit(WsEvent::Disconnect);
                            }
                            ws = WebSocket::open(&url).expect("woof");
                            info!("ws: Reconnected to websocket.");
                            for topic in subscriptions.keys() {
                                let command = WsCommand::Subscribe {
                                    topic: topic.clone(),
                                };
                                info!("ws: Resubscribing to {}", topic);
                                ws.send(Message::Text(serde_json::to_string(&command).unwrap()))
                                    .await
                                    .expect("meow");
                            }
                            info!("ws: Resubscribed to topics.");
                            for handler in &event_handlers {
                                handler.emit(WsEvent::Connect);
                            }
                            is_connected = true;
                            info!("ws: Reconnected to websocket and resubscribed.");
                        } else {
                            debug!("ws: Sending keep alive.");
                            let command = WsCommand::KeepAlive;
                            ws.send(Message::Text(serde_json::to_string(&command).unwrap()))
                                .await
                                .unwrap();
                        }

                        timeout = schedule_keep_alive(&in_tx_clone, keep_alive_duration_millis);
                    }
                    Either::Left((None, _)) => {
                        error!("ws: Command channel closed, quitting.");
                        break;
                    }
                    Either::Right((Some(Ok(msg)), _)) => {
                        debug!("ws: Received message: {:?}", msg);
                        if let Some(msg) = message_to_string(msg) {
                            let msg: MqttMessage = serde_json::from_str(&msg).unwrap();
                            if let Some(callbacks) = subscriptions.get(&msg.topic) {
                                for callback in callbacks {
                                    callback.emit(msg.clone());
                                }
                            }
                        }
                        timeout = schedule_keep_alive(&in_tx_clone, keep_alive_duration_millis);
                    }
                    Either::Right((Some(Err(err)), _)) => {
                        error!("ws: Failed to receive message: {:?}, reconnecting.", err);
                        is_connected = false;
                        for handler in &event_handlers {
                            handler.emit(WsEvent::Disconnect);
                        }
                        timeout = schedule_keep_alive(&in_tx_clone, reconnect_delay_millis);
                    }
                    Either::Right((None, _)) => {
                        error!("ws: closed, reconnecting.");
                        is_connected = false;
                        for handler in &event_handlers {
                            handler.emit(WsEvent::Disconnect);
                        }
                        timeout = schedule_keep_alive(&in_tx_clone, reconnect_delay_millis);
                    }
                }
            }
        });

        Self { tx: in_tx }
    }
}

fn schedule_keep_alive(in_tx: &Sender<Command>, millis: u32) -> Option<Timeout> {
    debug!("Scheduling next keepalive");
    let mut in_tx_clone = in_tx.clone();
    Some(Timeout::new(millis, move || {
        in_tx_clone.try_send(Command::KeepAlive).unwrap();
    }))
}

fn get_websocket_url() -> String {
    let window = window().unwrap();
    let location = window.location();
    let protocol = if location.protocol().unwrap() == "https:" {
        "wss"
    } else {
        "ws"
    };
    let host = location.host().unwrap();
    format!("{}://{}/websocket", protocol, host)
}
