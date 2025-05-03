use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::{IntoResponse, Response},
};
use futures::{stream::FuturesUnordered, StreamExt};
use tokio::select;
use tower_sessions::Session;
use tracing::{debug, error};

use robotica_common::{
    mqtt::MqttMessage,
    protobuf::ProtobufEncoderDecoder,
    version::Version,
    websocket::{WsCommand, WsError, WsStatus},
};

use crate::{
    pipes::{stateful, Subscriber, Subscription},
    services::mqtt::{topics::topic_matches_any, MqttTx},
};

use super::{get_user, Config, User};

#[allow(clippy::unused_async)]
pub(super) async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(config): State<Arc<Config>>,
    State(mqtt): State<MqttTx>,
    session: Session,
) -> Response {
    #[allow(clippy::option_if_let_else)]
    if let Some(user) = get_user(&session).await {
        debug!("Accessing websocket");
        ws.on_upgrade(|socket| websocket(socket, config, user, mqtt))
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

// FIXME: function is too long
#[allow(clippy::too_many_lines)]
async fn websocket(mut stream: WebSocket, config: Arc<Config>, user: User, mqtt: MqttTx) {
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

    if let Err(e) = stream.send(Message::Binary(message.into())).await {
        error!("Error sending websocket status: {}", e);
    }

    let mut add_subscriptions: Vec<String> = Vec::new();
    let mut remove_subscriptions: Vec<String> = Vec::new();
    let mut subscriptions: HashMap<String, stateful::Subscription<MqttMessage>> = HashMap::new();

    loop {
        for topic in &add_subscriptions {
            let already_subscribed = subscriptions.contains_key(topic);
            if !already_subscribed {
                match mqtt.subscribe_into_stateful(topic).await {
                    Ok(entity) => {
                        debug!("Subscribed to topic: {}", topic);
                        let subscription = entity.subscribe().await;
                        subscriptions.insert(topic.clone(), subscription);
                    }
                    Err(e) => {
                        error!("Failed to subscribe to topic: {}", e);
                    }
                }
            }
        }
        add_subscriptions.clear();

        for topic in &remove_subscriptions {
            if let Some(_subscription) = subscriptions.remove(topic) {
                debug!("Unsubscribed from topic: {}", topic);
            }
        }
        remove_subscriptions.clear();

        let mut futures = {
            let futures = FuturesUnordered::new();
            for s in subscriptions.values_mut() {
                futures.push(s.recv());
            }
            futures
        };

        select! {
            Some(msg) = futures.next() => {
                    let msg = match msg {
                        Ok(msg) => msg,
                        Err(e) => {
                            error!("websocket: failed to receive message from broadcast: {}", e);
                            continue;
                        }
                    };

                    let msg = match msg.encode() {
                        Ok(msg) => msg,
                        Err(e) => {
                            error!("websocket: failed to serialize message: {}", e);
                            continue;
                        }
                    };

                    if let Err(err) = stream.send(Message::Binary(msg.into())).await {
                        error!("websocket: failed to send message to web socket, stopping: {}", err);
                        break;
                    }
            }
            msg = stream.next() => {
                let msg = match msg {
                    Some(Ok(Message::Text(msg))) => {
                        error!("websocket: received text message, ignoring: {}", msg);
                        continue;
                    },
                    Some(Ok(Message::Binary(msg))) => {
                        WsCommand::decode(&msg)
                    }
                    Some(Ok(Message::Close(_))) => {
                        debug!("websocket: received close message, stopping");
                        break;
                    }
                    Some(Ok(msg)) => {
                        debug!("websocket: received unexpected message from web socket: {:?}", msg);
                        continue;
                    }
                    Some(Err(err)) => {
                        debug!("websocket: failed to receive message from web socket, stopping: {}", err);
                        break;
                    }
                    None => {
                        debug!("websocket: websocket closed, stopping");
                        break;
                    }
                };
                match msg {
                    Ok(WsCommand::Subscribe { topic }) => {
                        debug!("websocket: Received subscribe command: {}", topic);
                        // We cannot touch the subscriptions map directly, because it is borrowed mutably by the futures.
                        if check_topic_subscribe_allowed(&topic, &user, &config) {
                            add_subscriptions.push(topic);
                        }
                    }
                    Ok(WsCommand::Unsubscribe { topic }) => {
                        debug!("websocket: Received unsubscribe command: {}", topic);
                        // We cannot touch the subscriptions map directly, because it is borrowed mutably by the futures.
                        remove_subscriptions.push(topic);
                    }
                    Ok(WsCommand::Send(msg)) => {
                        if check_topic_send_allowed(&msg.topic, &user, &config) {
                            tracing::info!("websocket: Sending message to mqtt: {:?}", msg);
                            mqtt.try_send(msg);
                        }
                    }
                    Ok(WsCommand::KeepAlive) => {
                        // Do nothing
                    }
                    Err(err) => {
                        tracing::error!("websocket: Error parsing message: {}", err);
                    }
                }
            },
        }
    }

    debug!("websocket: ending socket");
}

// FIXME: Revise
const ADMIN_SUBSCRIBE_TOPICS: &[&str] = &[];

// FIXME: Revise
const ALLOWED_SUBSCRIBE_TOPICS: &[&str] = &[
    "state/#",
    "stat/#",
    "tele/#",
    "schedule/#",
    "robotica/#",
    "zigbee2mqtt/#",
    "zwave/#",
    "workgroup/#",
];
const ALLOWED_SEND_TOPICS: &[&str] = &["command/#", "robotica/command/#", "cmnd/#", "zwave/#"];

#[must_use]
#[allow(clippy::cognitive_complexity)]
fn check_topic_subscribe_allowed(topic: &str, user: &User, _config: &Config) -> bool {
    let admin_required = topic_matches_any(topic, ADMIN_SUBSCRIBE_TOPICS);
    let user_allowed = topic_matches_any(topic, ALLOWED_SUBSCRIBE_TOPICS);

    match (admin_required, user_allowed) {
        (true, _) if user.is_admin => {
            debug!(
                "check_topic_subscribe_allowed: Topic {} allowed as admin",
                topic
            );
            true
        }
        (true, _) => {
            error!(
                "check_topic_subscribe_allowed: Topic {} not allowed as not admin",
                topic
            );
            false
        }
        (false, true) => {
            debug!("check_topic_subscribe_allowed: topic {} allowed", topic);
            true
        }
        (false, false) => {
            error!("check_topic_subscribe_allowed: Topic {} not allowed", topic);
            false
        }
    }
}

#[must_use]
fn check_topic_send_allowed(topic: &str, _user: &User, _config: &Config) -> bool {
    if topic_matches_any(topic, ALLOWED_SEND_TOPICS) {
        debug!("check_topic_send_allowed: topic {} allowed", topic);
        true
    } else {
        error!("check_topic_send_allowed: topic {} not allowed", topic);
        false
    }
}
