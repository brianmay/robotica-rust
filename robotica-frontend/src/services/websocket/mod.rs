//! Websocket service for robotica frontend.
use std::collections::HashMap;

use futures::{
    channel::{mpsc::Sender, oneshot},
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
    mqtt::MqttMessage,
    user::User,
    version::Version,
    websocket::{WsCommand, WsConnect, WsError},
};

/// A websocket subscription
///
/// When this is dropped, the subscription will be cancelled.
#[derive(Debug)]
pub struct Subscription {
    id: usize,
    tx: Sender<Command>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.tx
            .try_send(Command::Unsubscribe(self.id))
            .unwrap_or_else(|e| {
                error!("Error unsubscribing from websocket: {}", e);
            });
    }
}

/// A websocket command, sent to the websocket service.
#[derive(Debug)]
enum Command {
    /// Subscribe to a MQTT topic.
    Subscribe {
        /// MQTT topic to subscribe to.
        to: SubscribeTo,
        /// Callback to call when a message is received.
        // callback: Callback<MqttMessage>,
        /// Channel to return the subscription
        tx: oneshot::Sender<Subscription>,
    },
    /// Unsubscribe from a MQTT topic.
    Unsubscribe(usize),
    /// Callback to call when a connect or disconnect event occurs.
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

#[derive(Clone)]
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
}

/// The websocket service
#[derive(Clone)]
pub struct WebsocketService {
    /// Channel to send commands to the websocket service.
    tx: Sender<Command>,
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
    Disconnected,
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

#[derive(Clone, Debug)]
enum SubscribeTo {
    Mqtt(String, Callback<MqttMessage>),
    Events(Callback<WsEvent>),
}

struct Subscriptions {
    next_id: usize,
    subscriptions: HashMap<usize, SubscribeTo>,
}

impl Subscriptions {
    fn new() -> Self {
        Self {
            next_id: 0,
            subscriptions: HashMap::new(),
        }
    }

    fn subscribe(&mut self, to: &SubscribeTo) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.subscriptions.insert(id, to.clone());
        id
    }

    fn unsubscribe(&mut self, id: usize) -> Option<SubscribeTo> {
        self.subscriptions.remove(&id)
    }

    fn get_mqtt_topics(&self) -> Vec<&str> {
        self.subscriptions
            .values()
            .filter_map(|to| match to {
                SubscribeTo::Mqtt(topic, ..) => Some(topic),
                SubscribeTo::Events(..) => None,
            })
            .map(std::string::String::as_str)
            .collect()
    }

    fn is_mqtt_topic_subscribed(&self, topic: &str) -> bool {
        self.subscriptions
            .values()
            .filter_map(|to| match to {
                SubscribeTo::Mqtt(topic, ..) => Some(topic),
                SubscribeTo::Events(..) => None,
            })
            .any(|t| t == topic)
    }

    fn dispatch_mqtt(&self, msg: &MqttMessage) {
        self.subscriptions
            .values()
            .filter_map(|to| match to {
                SubscribeTo::Mqtt(topic, callback) if *topic == msg.topic => Some(callback),
                _ => None,
            })
            .for_each(|callback: &Callback<MqttMessage>| {
                callback.emit(msg.clone());
            });
    }

    fn dispatch_event(&self, event: &WsEvent) {
        self.subscriptions
            .values()
            .filter_map(|to| match to {
                SubscribeTo::Events(callback) => Some(callback),
                SubscribeTo::Mqtt(..) => None,
            })
            .for_each(|callback: &Callback<WsEvent>| {
                callback.emit(event.clone());
            });
    }
}

struct State {
    url: String,
    subscriptions: Subscriptions,
    backend: BackendState,
    timeout: Option<Timeout>,
    in_tx: Sender<Command>,
    last_mqtt: HashMap<String, MqttMessage>,
    last_event: Option<WsEvent>,
}

impl State {
    fn dispatch_mqtt(&mut self, msg: &MqttMessage) {
        self.last_mqtt.insert(msg.topic.clone(), msg.clone());
        self.subscriptions.dispatch_mqtt(msg);
    }

    fn dispatch_event(&mut self, event: &WsEvent) {
        self.last_event = Some(event.clone());
        self.subscriptions.dispatch_event(event);
    }

    fn dispatch_last_mqtt(&self, topic: &str) {
        if let Some(msg) = self.last_mqtt.get(topic) {
            self.subscriptions.dispatch_mqtt(msg);
        }
    }

    fn dispatch_last_event(&self) {
        if let Some(event) = &self.last_event {
            self.subscriptions.dispatch_event(event);
        }
    }
}

impl WebsocketService {
    /// Create a new websocket service.
    #[must_use]
    pub fn new() -> Self {
        let url = get_websocket_url();
        info!("Connecting to {}", url);

        let (in_tx, mut in_rx) = futures::channel::mpsc::channel::<Command>(50);

        let mut state = State {
            url,
            subscriptions: Subscriptions::new(),
            backend: BackendState::Disconnected,
            timeout: None,
            in_tx: in_tx.clone(),
            last_mqtt: HashMap::new(),
            last_event: None,
        };

        spawn_local(async move {
            reconnect_and_set_keep_alive(&mut state).await;

            loop {
                if let BackendState::Connected(backend) = &mut state.backend {
                    match select(in_rx.next(), backend.ws.next()).await {
                        Either::Left((command, _)) => {
                            if process_command(command, &mut state).await.is_stop() {
                                break;
                            }
                        }
                        Either::Right((msg, _)) => {
                            process_message(msg, &mut state);
                        }
                    }
                } else {
                    let command = in_rx.next().await;
                    if process_command(command, &mut state).await.is_stop() {
                        break;
                    }
                };
            }
        });

        Self { tx: in_tx }
    }

    fn command(&mut self, command: Command) {
        if let Err(err) = self.tx.try_send(command) {
            error!("ws: Failed to send command: {:?}", err);
        }
    }

    /// Send an outgoing MQTT message
    pub fn send_mqtt(&mut self, msg: MqttMessage) {
        self.command(Command::Send(msg));
    }

    /// Subscribe to a MQTT topic
    ///
    /// # Panics
    ///
    /// Panics if the servers fails to return the Subscription
    pub async fn subscribe_mqtt(
        &mut self,
        topic: String,
        callback: Callback<MqttMessage>,
    ) -> Subscription {
        let (tx, rx) = oneshot::channel();

        self.command(Command::Subscribe {
            to: SubscribeTo::Mqtt(topic, callback),
            tx,
        });

        rx.await.unwrap()
    }

    /// Subscribe to events
    ///
    /// # Panics
    ///
    /// Panics if the servers fails to return the Subscription
    pub async fn subscribe_events(&mut self, callback: Callback<WsEvent>) -> Subscription {
        let (tx, rx) = oneshot::channel();

        self.command(Command::Subscribe {
            to: SubscribeTo::Events(callback),
            tx,
        });

        rx.await.unwrap()
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

enum ProcessCommandResult {
    Stop,
    Continue,
}

impl ProcessCommandResult {
    const fn is_stop(&self) -> bool {
        matches!(self, ProcessCommandResult::Stop)
    }
}

async fn process_command(command: Option<Command>, state: &mut State) -> ProcessCommandResult {
    let is_connected = state.backend.is_connected();

    match command {
        Some(Command::Subscribe { to, tx }) => {
            debug!("ws: Subscribing to {:?}", to);

            if let SubscribeTo::Mqtt(topic, ..) = &to {
                if !state.subscriptions.is_mqtt_topic_subscribed(topic) {
                    let command = WsCommand::Subscribe {
                        topic: topic.clone(),
                    };
                    state
                        .backend
                        .send(Message::Text(serde_json::to_string(&command).unwrap()))
                        .await;
                    set_timeout(&mut state.timeout, &state.in_tx, KEEP_ALIVE_DURATION_MILLIS);
                }
            }

            let id = state.subscriptions.subscribe(&to);
            tx.send(Subscription {
                id,
                tx: state.in_tx.clone(),
            })
            .unwrap_or_else(|s| {
                error!("ws: Failed to send subscription request to backend: {s:?}");
            });

            match &to {
                SubscribeTo::Mqtt(topic, _) => state.dispatch_last_mqtt(topic),
                SubscribeTo::Events(_) => state.dispatch_last_event(),
            }

            debug!("ws: subscribed to {:?}", to);
            ProcessCommandResult::Continue
        }
        Some(Command::Unsubscribe(id)) => {
            debug!("ws: Unsubscribing from {}", id);
            let to = state.subscriptions.unsubscribe(id);

            if let Some(SubscribeTo::Mqtt(topic, ..)) = to {
                if state.subscriptions.is_mqtt_topic_subscribed(&topic) {
                    // FIXME: We don't have an unsubscribe command in the backend yet.
                    // let command = WsCommand::Unsubscribe { id };
                    // state
                    //     .backend
                    //     .send(Message::Text(serde_json::to_string(&command).unwrap()))
                    //     .await;
                    // set_timeout(&mut state.timeout, &state.in_tx, KEEP_ALIVE_DURATION_MILLIS);
                    // state.last_message.remove(&topic);
                }
            }

            debug!("ws: unsubscribed from {}", id);
            ProcessCommandResult::Continue
        }
        Some(Command::Send(msg)) => {
            debug!("ws: Sending message: {:?}", msg);
            let command = WsCommand::Send(msg);
            state
                .backend
                .send(Message::Text(serde_json::to_string(&command).unwrap()))
                .await;
            set_timeout(&mut state.timeout, &state.in_tx, KEEP_ALIVE_DURATION_MILLIS);
            ProcessCommandResult::Continue
        }
        Some(Command::KeepAlive) => {
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
            ProcessCommandResult::Continue
        }
        None => {
            debug!("ws: Got Close command.");
            state.timeout = None;
            state.backend = BackendState::Disconnected;
            state.dispatch_event(&WsEvent::Disconnected("Closed".to_string()));
            ProcessCommandResult::Stop
        }
    }
}

fn process_message(msg: Option<Result<Message, WebSocketError>>, state: &mut State) {
    match msg {
        Some(Ok(msg)) => {
            debug!("ws: Received message: {:?}", msg);
            if let Some(msg) = message_to_string(msg) {
                let msg: MqttMessage = serde_json::from_str(&msg).unwrap();
                state.dispatch_mqtt(&msg);
            }
            set_timeout(&mut state.timeout, &state.in_tx, KEEP_ALIVE_DURATION_MILLIS);
        }
        Some(Err(err)) => {
            error!("ws: Failed to receive message: {:?}, reconnecting.", err);
            state.backend = BackendState::Disconnected;
            state.dispatch_event(&WsEvent::Disconnected(err.to_string()));
            set_timeout(&mut state.timeout, &state.in_tx, RECONNECT_DELAY_MILLIS);
        }
        None => {
            error!("ws: closed, reconnecting.");
            let msg = "Connection closed";
            state.backend = BackendState::Disconnected;
            state.dispatch_event(&WsEvent::Disconnected(msg.to_string()));
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
            state.dispatch_event(&WsEvent::Connected {
                user: backend.user.clone(),
                version: backend.version.clone(),
            });
            state.backend = BackendState::Connected(backend);
        }
        Err(ConnectError::RetryableError(err)) => {
            error!("ws: Failed to reconnect: {:?}, retrying.", err);
            set_timeout(&mut state.timeout, &state.in_tx, RECONNECT_DELAY_MILLIS);
            state.backend = BackendState::Disconnected;
            state.dispatch_event(&WsEvent::Disconnected(err.to_string()));
        }
        Err(ConnectError::FatalError(err)) => {
            error!("ws: Failed to reconnect: {}, not retrying", err);
            state.timeout = None;
            state.backend = BackendState::Disconnected;
            state.dispatch_event(&WsEvent::Disconnected(err.to_string()));
        }
    }
}

async fn reconnect(url: &str, subscriptions: &Subscriptions) -> Result<Backend, ConnectError> {
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

    let topics = subscriptions.get_mqtt_topics();
    for topic in topics {
        let command = WsCommand::Subscribe {
            topic: topic.to_string(),
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
    format!("{protocol}://{host}/websocket")
}
