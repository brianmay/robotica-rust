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

pub struct WebsocketService {
    pub tx: Sender<WsCommand>,
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
    pub fn new(callback: Callback<MqttMessage>) -> Self {
        let url = get_websocket_url();
        info!("Connecting to {}", url);

        let mut ws = WebSocket::open(&url).unwrap();

        let (in_tx, mut in_rx) = futures::channel::mpsc::channel::<WsCommand>(10);

        spawn_local(async move {
            loop {
                match select(in_rx.next(), ws.next()).await {
                    Either::Left((Some(cmd), _)) => {
                        serde_json::to_string(&cmd)
                            .map(Message::Text)
                            .map(|m| ws.send(m))
                            .unwrap()
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
                            callback.emit(msg);
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
