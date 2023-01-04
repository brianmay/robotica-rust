use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::{IntoResponse, Response},
};
use axum_sessions::extractors::ReadableSession;
use futures::{SinkExt, StreamExt};
use tokio::{select, sync::mpsc};
use tracing::{debug, error};

use robotica_common::{
    mqtt::MqttMessage,
    protobuf::ProtobufEncoderDecoder,
    version::Version,
    websocket::{WsCommand, WsError, WsStatus},
};

use crate::services::mqtt::topics::topic_matches_any;

use super::{get_user, HttpConfig, User};

#[allow(clippy::unused_async)]
pub(super) async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(config): State<HttpConfig>,
    session: ReadableSession,
) -> Response {
    #[allow(clippy::option_if_let_else)]
    if let Some(user) = get_user(&session) {
        debug!("Accessing websocket");
        ws.on_upgrade(|socket| websocket(socket, config, user))
            .into_response()
    } else {
        error!("Permission denied to websocket");
        ws.on_upgrade(|socket| websocket_error(socket, WsError::NotAuthorized))
            .into_response()
    }
}

async fn websocket_error(stream: WebSocket, error: WsError) {
    let mut stream = stream;
    let message = WsStatus::Disconnected(error);
    let message = match message.encode() {
        Ok(msg) => msg,
        Err(e) => {
            error!("Error encoding websocket error: {}", e);
            return;
        }
    };
    if let Err(e) = stream.send(Message::Binary(message.into())).await {
        error!("Error sending websocket error: {}", e);
    }
}

async fn websocket(stream: WebSocket, config: HttpConfig, user: User) {
    // By splitting we can send and receive at the same time.
    let (mut sender, mut receiver) = stream.split();

    // Send Connect message.
    let message = WsStatus::Connected {
        user: user.clone(),
        version: Version::get(),
    };

    let message = match message.encode() {
        Ok(msg) => msg,
        Err(e) => {
            error!("Error encoding websocket error: {}", e);
            return;
        }
    };

    if let Err(e) = sender.send(Message::Binary(message.into())).await {
        error!("Error sending websocket status: {}", e);
    }

    // We can't clone sender, so we create a process that can receive from multiple threads.
    let (tx, rx) = mpsc::unbounded_channel::<MqttMessage>();
    let send_task = tokio::spawn(async move {
        debug!("send_task: starting send_task");

        let mut rx = rx;
        while let Some(msg) = rx.recv().await {
            let msg = match msg.encode() {
                Ok(msg) => msg,
                Err(e) => {
                    error!("send_task: failed to serialize message: {}", e);
                    continue;
                }
            };

            if let Err(err) = sender.send(Message::Binary(msg.into())).await {
                error!(
                    "send_task: failed to send message to web socket, stopping: {}",
                    err
                );
                break;
            }

            // Note: sender.closed() is not implemented, so
            // we can't check if the socket is closed.
            // Instead we kill this process when the recv_task process dies.
        }

        debug!("send_task: stopping");
    });

    // This task will receive messages from client and send them to broadcast subscribers.
    let recv_task = tokio::spawn(async move {
        debug!("recv_task: starting recv_task");

        let tx = tx;

        loop {
            select! {
                msg = receiver.next() => {
                    let msg = match msg {
                        Some(Ok(Message::Text(msg))) => {
                            error!("recv_task: received text message, ignoring: {}", msg);
                            continue;
                        },
                        Some(Ok(Message::Binary(msg))) => {
                            WsCommand::decode(&msg)
                        }
                        Some(Ok(Message::Close(_))) => {
                            debug!("recv_task: received close message, stopping");
                            break;
                        }
                        Some(Ok(msg)) => {
                            debug!("recv_task: received unexpected message from web socket: {:?}", msg);
                            continue;
                        }
                        Some(Err(err)) => {
                            debug!("recv_task: failed to receive message from web socket, stopping: {}", err);
                            break;
                        }
                        None => {
                            debug!("recv_task: websocket closed, stopping");
                            break;
                        }
                    };
                    match msg {
                        Ok(WsCommand::Subscribe { topic }) => {
                            if check_topic_subscribe_allowed(&topic, &user, &config) {
                                process_subscribe(topic, &config, tx.clone()).await;
                            }
                        }
                        Ok(WsCommand::Send(msg)) => {
                            if check_topic_send_allowed(&msg.topic, &user, &config) {
                                tracing::info!("recv_task: Sending message to mqtt {}: {}", msg.topic, msg.payload);
                                config.mqtt.try_send(msg);
                            }
                        }
                        Ok(WsCommand::KeepAlive) => {
                            // Do nothing
                        }
                        Err(err) => {
                            tracing::error!("recv_task: Error parsing message: {}", err);
                        }
                    };
                },
                _ = tx.closed() => {
                    debug!("recv_task: send_task pipe closed, stopping");
                    break;
                }
            }
        }

        debug!("recv_task: ending recv_task");
    });

    let _rc = recv_task.await;
    send_task.abort();
}

const ALLOWED_SUBSCRIBE_TOPICS: &[&str] = &[
    "state/#",
    "stat/#",
    "tele/#",
    "schedule/#",
    "robotica/#",
    "zigbee2mqtt/#",
    "workgroup/#",
];
const ALLOWED_SEND_TOPICS: &[&str] = &["command/#", "cmnd/#"];

#[must_use]
fn check_topic_subscribe_allowed(topic: &str, _user: &User, _config: &HttpConfig) -> bool {
    let topics = ALLOWED_SUBSCRIBE_TOPICS
        .iter()
        .map(std::string::ToString::to_string);
    if topic_matches_any(topic, topics) {
        debug!("check_topic_subscribe_allowed: topic {} allowed", topic);
        true
    } else {
        error!("check_topic_subscribe_allowed: Topic {} not allowed", topic);
        false
    }
}

#[must_use]
fn check_topic_send_allowed(topic: &str, _user: &User, _config: &HttpConfig) -> bool {
    let topics = ALLOWED_SEND_TOPICS
        .iter()
        .map(std::string::ToString::to_string);
    if topic_matches_any(topic, topics) {
        debug!("check_topic_send_allowed: topic {} allowed", topic);
        true
    } else {
        error!("check_topic_send_allowed: topic {} not allowed", topic);
        false
    }
}

async fn process_subscribe(
    topic: String,
    config: &HttpConfig,
    sender: mpsc::UnboundedSender<MqttMessage>,
) {
    debug!("Subscribing to {}", topic);
    let rc = config.mqtt.subscribe(&topic).await;
    let rx = match rc {
        Ok(rx) => rx,
        Err(e) => {
            error!("Error subscribing to {}: {}", topic, e);
            return;
        }
    };

    let rx_s = rx.subscribe().await;
    let topic_clone = topic.clone();
    tokio::spawn(async move {
        debug!("Starting receiver for {}", topic_clone);
        let mut rx_s = rx_s;
        loop {
            select! {
                Ok(msg) = rx_s.recv() => {
                    if let Err(err) = sender.send(msg) {
                        error!("topic_task: Error sending MQTT message: {}, unsubscribing from {}", err, topic_clone);
                        break;
                    }
                }
                _ = sender.closed() => {
                    debug!("topic_task: send_task pipe closed, unsubscribing from {}", topic_clone);
                    break;
                }
            }
        }
        debug!("topic_task: Ending receiver for {}", topic_clone);
    });
}
