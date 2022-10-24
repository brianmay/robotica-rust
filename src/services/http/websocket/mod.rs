mod protocol;

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket},
        WebSocketUpgrade,
    },
    response::{IntoResponse, Response},
    Extension,
};
use axum_sessions::extractors::ReadableSession;
use futures::{SinkExt, StreamExt};
use reqwest::StatusCode;
use tokio::{select, sync::mpsc};
use tracing::{debug, error, info};

use crate::services::mqtt::{self, topics::topic_matches_any};

use self::protocol::{MqttMessage, WsCommand};
use super::{get_user, HttpConfig, User};

#[allow(clippy::unused_async)]
pub(super) async fn websocket_handler(
    ws: WebSocketUpgrade,
    Extension(config): Extension<Arc<HttpConfig>>,
    session: ReadableSession,
) -> Response {
    #[allow(clippy::option_if_let_else)]
    if let Some(user) = get_user(&session) {
        info!("Accessing websocket");
        ws.on_upgrade(|socket| websocket(socket, config, user))
            .into_response()
    } else {
        error!("Permission denied to websocket");
        Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::empty())
            .unwrap()
            .into_response()
    }
}

async fn websocket(stream: WebSocket, config: Arc<HttpConfig>, user: User) {
    // By splitting we can send and receive at the same time.
    let (mut sender, mut receiver) = stream.split();

    // We can't clone sender, so we create a process that can receive from multiple threads.
    let (tx, rx) = mpsc::unbounded_channel::<MqttMessage>();
    let send_task = tokio::spawn(async move {
        debug!("send_task: starting send_task");

        let mut rx = rx;
        while let Some(msg) = rx.recv().await {
            let msg = match serde_json::to_string(&msg) {
                Ok(msg) => msg,
                Err(e) => {
                    error!("send_task: failed to serialize message: {}", e);
                    continue;
                }
            };

            if let Err(err) = sender.send(Message::Text(msg)).await {
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
                        Some(Ok(Message::Text(msg))) => msg,
                        Some(Ok(Message::Binary(_))) => {
                            // FIXME: Implement binary messages
                            error!("recv_task: received binary message, ignoring");
                            continue;
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
                            error!("recv_task: failed to receive message from web socket, stopping: {}", err);
                            break;
                        }
                        None => {
                            error!("recv_task: failed to receive message from web socket, stopping");
                            break;
                        }
                    };
                    let msg: Result<WsCommand, _> = serde_json::from_str(&msg);
                    match msg {
                        Ok(WsCommand::Subscribe { topic }) => {
                            if check_topic_subscribe_allowed(&topic, &user, &config) {
                                process_subscribe(topic, &config, tx.clone()).await;
                            }
                        }
                        Ok(WsCommand::Send(msg)) => {
                            if check_topic_send_allowed(&msg.topic, &user, &config) {
                                tracing::info!("recv_task: Sending message to mqtt {}: {}", msg.topic, msg.payload);
                                let message: mqtt::Message = msg.into();
                                config.mqtt.try_send(message);
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

const ALLOWED_SUBSCRIBE_TOPICS: &[&str] = &["state/#"];
const ALLOWED_SEND_TOPICS: &[&str] = &["command/#"];

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
    config: &Arc<HttpConfig>,
    sender: mpsc::UnboundedSender<MqttMessage>,
) {
    info!("Subscribing to {}", topic);
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
                    let msg: MqttMessage = match msg.try_into() {
                        Ok(msg) => msg,
                        Err(e) => {
                            error!("topic_task: Error converting message: {}", e);
                            continue
                        }
                    };
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
