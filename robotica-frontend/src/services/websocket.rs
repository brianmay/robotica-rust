use std::collections::HashMap;

use futures::{
    channel::mpsc::Sender,
    future::{select, Either},
    SinkExt, StreamExt,
};
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
    Send(MqttMessage),
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

        let mut ws = WebSocket::open(&url).unwrap();
        let mut subscriptions: HashMap<String, Vec<Callback<MqttMessage>>> = HashMap::new();

        let (in_tx, mut in_rx) = futures::channel::mpsc::channel::<Command>(10);

        spawn_local(async move {
            loop {
                match select(in_rx.next(), ws.next()).await {
                    Either::Left((Some(Command::Subscribe { topic, callback }), _)) => {
                        match subscriptions.get_mut(&topic) {
                            Some(callbacks) => callbacks.push(callback),
                            None => {
                                subscriptions.insert(topic.clone(), vec![callback]);
                                let command = WsCommand::Subscribe { topic };
                                ws.send(Message::Text(serde_json::to_string(&command).unwrap()))
                                    .await
                                    .unwrap();
                            }
                        }
                    }
                    Either::Left((Some(Command::Send(msg)), _)) => {
                        let command = WsCommand::Send(msg);
                        ws.send(Message::Text(serde_json::to_string(&command).unwrap()))
                            .await
                            .unwrap();
                    }
                    Either::Left((None, _)) => {
                        error!("Command channel closed");
                        break;
                    }
                    Either::Right((Some(Ok(msg)), _)) => {
                        if let Some(msg) = message_to_string(msg) {
                            debug!("Received message: {:?}", msg);
                            let msg: MqttMessage = serde_json::from_str(&msg).unwrap();
                            if let Some(callbacks) = subscriptions.get(&msg.topic) {
                                for callback in callbacks {
                                    callback.emit(msg.clone());
                                }
                            }
                        }
                    }
                    Either::Right((Some(Err(err)), _)) => {
                        error!("Failed to receive message: {:?}", err);
                        break;
                    }
                    Either::Right((None, _)) => {
                        error!("Websocket closed");
                        break;
                    }
                }
            }
        });

        Self { tx: in_tx }
    }
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
