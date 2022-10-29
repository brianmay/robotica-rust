//! Websocket service for robotica frontend.
use std::collections::HashMap;

use futures::{
    channel::mpsc::Sender,
    future::{select, Either},
    SinkExt, StreamExt,
};
use gloo_timers::callback::Timeout;
use log::{debug, error, info};
use reqwasm::websocket::{futures::WebSocket, Message, WebSocketError};
use thiserror::Error;
use wasm_bindgen_futures::spawn_local;
use web_sys::window;
use yew::Callback;

use robotica_common::{
    user::User,
    version::Version,
    websocket::{MqttMessage, WsCommand, WsConnect, WsError},
};

/// A websocket command, sent to the websocket service.
#[derive(Debug)]
pub enum Command {
    /// Subscribe to a MQTT topic.
    Subscribe {
        /// MQTT topic to subscribe to.
        topic: String,
        /// Callback to call when a message is received.
        callback: Callback<MqttMessage>,
    },
    /// Callback to call when a connect or disconnect event occurs.
    EventHandler(Callback<WsEvent>),
    /// Send a MQTT message.
    Send(MqttMessage),
    /// Send a keep alive message.
    KeepAlive,
}

/// The details from the backend
struct Backend {
    /// The websocket to talk to the backend
    ws: WebSocket,

    /// The user on the backend
    user: User,

    /// The version of the backend
    version: Version,
}

/// A connect/disconnect event
pub enum WsEvent {
    /// Connected to the websocket server
    Connected {
        /// The user on the backend
        user: User,

        /// The version of the backend
        version: Version,
    },
    /// Disconnected from the websocket server, will retry
    Disconnected(String),
    /// Disconnected from the websocket server, will not retry
    FatalError(String),
}

/// The websocket service
#[derive(Clone)]
pub struct WebsocketService {
    /// Channel to send commands to the websocket service.
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

const KEEP_ALIVE_DURATION_MILLIS: u32 = 15_000;
const RECONNECT_DELAY_MILLIS: u32 = 5_000;

enum BackendState {
    /// Backend is connected
    Connected(Backend),
    /// Disconnected from the websocket server, will retry
    Disconnected(String),
    /// Disconnected from the websocket server, will not retry
    FatalError(String),
}

impl BackendState {
    const fn is_connected(&self) -> bool {
        matches!(self, BackendState::Connected(_))
    }

    async fn send(&mut self, msg: Message) {
        if let BackendState::Connected(backend) = self {
            backend.ws.send(msg).await.unwrap_or_else(|err| {
                error!("ws: Failed to send message: {:?}", err);
            });
        } else {
            error!("Discarding outgoing message as not connected");
        }
    }
}

struct State {
    url: String,
    subscriptions: HashMap<String, Vec<Callback<MqttMessage>>>,
    backend: BackendState,
    timeout: Option<Timeout>,
    in_tx: Sender<Command>,
    event_handlers: Vec<Callback<WsEvent>>,
}

impl WebsocketService {
    /// Create a new websocket service.
    #[must_use]
    pub fn new() -> Self {
        let url = get_websocket_url();
        info!("Connecting to {}", url);

        let (in_tx, mut in_rx) = futures::channel::mpsc::channel::<Command>(10);

        let mut state = State {
            url,
            subscriptions: HashMap::new(),
            backend: BackendState::Disconnected("Not connected".to_string()),
            timeout: None,
            in_tx: in_tx.clone(),
            event_handlers: vec![],
        };

        spawn_local(async move {
            reconnect_and_set_keep_alive(&mut state).await;

            loop {
                match &mut state.backend {
                    BackendState::Connected(backend) => {
                        match select(in_rx.next(), backend.ws.next()).await {
                            Either::Left((Some(command), _)) => {
                                process_command(command, &mut state).await;
                            }
                            Either::Left((None, _)) => {
                                error!("ws: Command channel closed, quitting.");
                                break;
                            }
                            Either::Right((msg, _)) => {
                                process_message(msg, &mut state);
                            }
                        }
                    }
                    _ => {
                        if let Some(command) = in_rx.next().await {
                            process_command(command, &mut state).await;
                        } else {
                            error!("ws: Command channel closed, quitting.");
                            break;
                        }
                    }
                };
            }
        });

        Self { tx: in_tx }
    }
}

impl Default for WebsocketService {
    fn default() -> Self {
        Self::new()
    }
}

/// `WebSocketError` does not implement `std::error::Error` so we wrap it.
#[derive(Debug)]
struct MyWebSocketError(WebSocketError);

impl std::error::Error for MyWebSocketError {}

impl std::fmt::Display for MyWebSocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WebSocketError: {:?}", self.0)
    }
}

// WebSocketError does not implement Error, so we wrap it.
#[derive(Error, Debug)]
enum FatalError {
    #[error("{0}")]
    Error(String),

    #[error("{0}")]
    AnyError(eyre::Error),
}

#[derive(Error, Debug)]
enum RetryableError {
    #[error("{0}")]
    AnyError(eyre::Error),
}

#[derive(Error, Debug)]
enum ConnectError {
    #[error("FatalError: {0}")]
    FatalError(#[from] FatalError),

    #[error("RetryableError: {0}")]
    RetryableError(#[from] RetryableError),
}

async fn process_command(command: Command, state: &mut State) {
    let is_connected = state.backend.is_connected();

    match command {
        Command::Subscribe { topic, callback } => {
            debug!("ws: Subscribing to {}", topic);
            if let Some(callbacks) = state.subscriptions.get_mut(&topic) {
                callbacks.push(callback);
            } else {
                state.subscriptions.insert(topic.clone(), vec![callback]);
                let command = WsCommand::Subscribe { topic };
                state
                    .backend
                    .send(Message::Text(serde_json::to_string(&command).unwrap()))
                    .await;
                set_timeout(&mut state.timeout, &state.in_tx, KEEP_ALIVE_DURATION_MILLIS);
            };
        }
        Command::Send(msg) => {
            debug!("ws: Sending message: {:?}", msg);
            let command = WsCommand::Send(msg);
            state
                .backend
                .send(Message::Text(serde_json::to_string(&command).unwrap()))
                .await;
            set_timeout(&mut state.timeout, &state.in_tx, KEEP_ALIVE_DURATION_MILLIS);
        }
        Command::EventHandler(handler) => {
            debug!("ws: Register event handler.");
            #[allow(clippy::option_if_let_else)]
            match &state.backend {
                BackendState::Connected(backend) => {
                    handler.emit(WsEvent::Connected {
                        user: backend.user.clone(),
                        version: backend.version.clone(),
                    });
                }
                BackendState::Disconnected(str) => {
                    handler.emit(WsEvent::Disconnected(str.clone()));
                }
                BackendState::FatalError(str) => {
                    handler.emit(WsEvent::FatalError(str.clone()));
                }
            }
            state.event_handlers.push(handler);
        }
        Command::KeepAlive => {
            debug!("ws: Got KeepAlive command.");
            state.timeout = None;

            if is_connected {
                debug!("ws: Sending keep alive.");
                let command = WsCommand::KeepAlive;
                state
                    .backend
                    .send(Message::Text(serde_json::to_string(&command).unwrap()))
                    .await;
                set_timeout(&mut state.timeout, &state.in_tx, KEEP_ALIVE_DURATION_MILLIS);
            } else {
                reconnect_and_set_keep_alive(state).await;
            }
        }
    };
}

fn process_message(msg: Option<Result<Message, WebSocketError>>, state: &mut State) {
    match msg {
        Some(Ok(msg)) => {
            debug!("ws: Received message: {:?}", msg);
            if let Some(msg) = message_to_string(msg) {
                let msg: MqttMessage = serde_json::from_str(&msg).unwrap();
                if let Some(callbacks) = state.subscriptions.get(&msg.topic) {
                    for callback in callbacks {
                        callback.emit(msg.clone());
                    }
                }
            }
            set_timeout(&mut state.timeout, &state.in_tx, KEEP_ALIVE_DURATION_MILLIS);
        }
        Some(Err(err)) => {
            error!("ws: Failed to receive message: {:?}, reconnecting.", err);
            state.backend = BackendState::Disconnected(err.to_string());
            for handler in &state.event_handlers {
                handler.emit(WsEvent::Disconnected(err.to_string()));
            }
            set_timeout(&mut state.timeout, &state.in_tx, RECONNECT_DELAY_MILLIS);
        }
        None => {
            error!("ws: closed, reconnecting.");
            let msg = "Connection closed";
            state.backend = BackendState::Disconnected(msg.to_string());
            for handler in &state.event_handlers {
                handler.emit(WsEvent::Disconnected(msg.to_string()));
            }
            set_timeout(&mut state.timeout, &state.in_tx, RECONNECT_DELAY_MILLIS);
        }
    }
}

async fn reconnect_and_set_keep_alive(state: &mut State) {
    let ws = reconnect(&state.url, &state.subscriptions).await;
    match ws {
        Ok(backend) => {
            info!("ws: Connected.");
            set_timeout(&mut state.timeout, &state.in_tx, KEEP_ALIVE_DURATION_MILLIS);
            for handler in &state.event_handlers {
                handler.emit(WsEvent::Connected {
                    user: backend.user.clone(),
                    version: backend.version.clone(),
                });
            }
            state.backend = BackendState::Connected(backend);
        }
        Err(ConnectError::RetryableError(err)) => {
            error!("ws: Failed to reconnect: {:?}, retrying.", err);
            set_timeout(&mut state.timeout, &state.in_tx, RECONNECT_DELAY_MILLIS);
            state.backend = BackendState::Disconnected(err.to_string());
            for handler in &state.event_handlers {
                handler.emit(WsEvent::Disconnected(err.to_string()));
            }
        }
        Err(ConnectError::FatalError(err)) => {
            error!("ws: Failed to reconnect: {}, not retrying", err);
            state.timeout = None;
            state.backend = BackendState::FatalError(err.to_string());
            for handler in &state.event_handlers {
                handler.emit(WsEvent::FatalError(err.to_string()));
            }
        }
    }
}

async fn reconnect(
    url: &str,
    subscriptions: &HashMap<String, Vec<Callback<MqttMessage>>>,
) -> Result<Backend, ConnectError> {
    info!("ws: Reconnecting to websocket.");
    let mut ws = WebSocket::open(url).map_err(|err| RetryableError::AnyError(err.into()))?;

    debug!("ws: Waiting for connected message.");
    let (user, version) = match ws.next().await {
        Some(Ok(msg)) => {
            if let Some(msg) = message_to_string(msg) {
                let msg: WsConnect =
                    serde_json::from_str(&msg).map_err(|err| FatalError::AnyError(err.into()))?;

                let our_version = Version::get();
                match msg {
                    WsConnect::Connected { user, version }
                        if version.vcs_ref == our_version.vcs_ref =>
                    {
                        info!("ws: Connected to websocket as user {}.", user.name);
                        (user, version)
                    }
                    WsConnect::Connected { version, .. } => {
                        info!(
                            "ws: Backend version {version} but frontend is {}.",
                            our_version
                        );
                        return Err(FatalError::Error(
                            "Backend version has changed; please reload".into(),
                        )
                        .into());
                    }
                    WsConnect::Disconnected(WsError::NotAuthorized) => {
                        info!("ws: Not authorized to connect to websocket.");
                        return Err(FatalError::Error("Not authorized".into()).into());
                    }
                }
            } else {
                return Err(
                    RetryableError::AnyError(eyre::eyre!("Connect message not received")).into(),
                );
            }
        }

        Some(Err(err)) => {
            debug!("ws: Failed to receive connected message: {:?}.", err);
            let err = MyWebSocketError(err);
            return Err(RetryableError::AnyError(err.into()).into());
        }
        None => {
            debug!("ws: Connection closed, waiting for connected message.");
            return Err(RetryableError::AnyError(eyre::eyre!("Websocket closed")).into());
        }
    };

    for topic in subscriptions.keys() {
        let command = WsCommand::Subscribe {
            topic: topic.clone(),
        };
        let msg =
            serde_json::to_string(&command).map_err(|err| FatalError::AnyError(err.into()))?;

        info!("ws: Resubscribing to {}", topic);
        ws.send(Message::Text(msg))
            .await
            .map_err(MyWebSocketError)
            .map_err(|err| RetryableError::AnyError(err.into()))?;
    }
    info!("ws: Resubscribed to topics.");

    let backend = Backend { ws, user, version };
    Ok(backend)
}

fn set_timeout(timeout: &mut Option<Timeout>, in_tx: &Sender<Command>, millis: u32) {
    debug!("Scheduling next keep alive");
    if let Some(timeout) = timeout.take() {
        timeout.cancel();
    }
    let mut in_tx_clone = in_tx.clone();
    *timeout = Some(Timeout::new(millis, move || {
        in_tx_clone.try_send(Command::KeepAlive).unwrap();
    }));
    debug!("Scheduling next keep alive.... done");
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
