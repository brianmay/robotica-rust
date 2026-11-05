use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;
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
    config::Config as UiConfig,
    mqtt::{MqttMessage, Topics},
    protobuf::ProtobufEncoderDecoder,
    user::User,
    version::Version,
    websocket::{WsCommand, WsError, WsStatus},
};

use crate::{
    pipes::{stateful, Subscriber, Subscription},
    services::mqtt::MqttTx,
};

use super::{get_user, oidc::Client};

#[allow(clippy::unused_async)]
pub(super) async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(ui_config): State<Option<Arc<UiConfig>>>,
    State(mqtt): State<MqttTx>,
    State(oidc_client): State<Arc<ArcSwap<Option<Client>>>>,
    session: Session,
) -> Response {
    #[allow(clippy::option_if_let_else)]
    if let Some(user) = get_user(&session).await {
        debug!("Accessing websocket");
        let allowed_topics = user_topics(&user, ui_config.as_ref());
        ws.on_upgrade(move |socket| websocket(socket, allowed_topics, user, mqtt))
            .into_response()
    } else {
        error!("Permission denied to websocket");
        let login_url = oidc_client
            .load()
            .as_ref()
            .as_ref()
            .map(|client| client.get_auth_url("/"));
        ws.on_upgrade(|socket| websocket_error(socket, WsError::NotAuthorized, login_url))
            .into_response()
    }
}

async fn websocket_error(stream: WebSocket, error: WsError, login_url: Option<String>) {
    let mut stream = stream;
    let message = WsStatus::Disconnected { error, login_url };
    let message = match message.encode() {
        Ok(msg) => msg,
        Err(e) => {
            error!("Error encoding websocket error: {}", e);
            return;
        }
    };
    if let Err(e) = stream.send(Message::Binary(message)).await {
        error!("Error sending websocket error: {}", e);
    }
}

// FIXME: function is too long
#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
async fn websocket(mut stream: WebSocket, allowed_topics: Topics, user: User, mqtt: MqttTx) {
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

    if let Err(e) = stream.send(Message::Binary(message)).await {
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

                    if let Err(err) = stream.send(Message::Binary(msg)).await {
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
                        if check_topic_subscribe_allowed(&topic, &allowed_topics) {
                            add_subscriptions.push(topic);
                        }
                    }
                    Ok(WsCommand::Unsubscribe { topic }) => {
                        debug!("websocket: Received unsubscribe command: {}", topic);
                        // We cannot touch the subscriptions map directly, because it is borrowed mutably by the futures.
                        remove_subscriptions.push(topic);
                    }
                    Ok(WsCommand::Send(msg)) => {
                        if check_topic_send_allowed(&msg.topic, &allowed_topics) {
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

/// Derive the MQTT topics a user is allowed to use.
///
/// The topics are derived from the ui views the user can access: the user
/// gets exactly the topics their views need, no more. When there is no ui
/// configuration, no topics are allowed (fail closed).
#[must_use]
fn user_topics(user: &User, ui_config: Option<&Arc<UiConfig>>) -> Topics {
    let Some(config) = ui_config else {
        error!("No ui configuration available; denying all mqtt topics");
        return Topics::default();
    };

    let groups: Vec<String> = user.groups.iter().map(|g| g.name.clone()).collect();
    config.filter_allowed(&groups).topics()
}

#[must_use]
fn check_topic_subscribe_allowed(topic: &str, allowed_topics: &Topics) -> bool {
    if allowed_topics.is_subscribe_allowed(topic) {
        debug!("check_topic_subscribe_allowed: topic {} allowed", topic);
        true
    } else {
        error!("check_topic_subscribe_allowed: Topic {} not allowed", topic);
        false
    }
}

#[must_use]
fn check_topic_send_allowed(topic: &str, allowed_topics: &Topics) -> bool {
    if allowed_topics.is_send_allowed(topic) {
        debug!("check_topic_send_allowed: topic {} allowed", topic);
        true
    } else {
        error!("check_topic_send_allowed: Topic {} not allowed", topic);
        false
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use robotica_common::config::{ControlsConfig, ViewConfig, ViewKind};
    use robotica_common::user::Group;

    fn user(group_names: &[&str]) -> User {
        User {
            id: 1,
            oidc_id: "oidc".to_string(),
            name: "Test".to_string(),
            email: "test@example.com".to_string(),
            is_admin: false,
            groups: group_names
                .iter()
                .zip(0i32..)
                .map(|(name, id)| Group {
                    id,
                    name: (*name).to_string(),
                })
                .collect(),
        }
    }

    fn ui_config() -> Arc<UiConfig> {
        let mut views = vec![
            ViewConfig {
                id: "guest".to_string(),
                title: "Guest".to_string(),
                menu: None,
                groups: vec![],
                kind: ViewKind::Tags,
            },
            ViewConfig {
                id: "family".to_string(),
                title: "Family".to_string(),
                menu: None,
                groups: vec!["family".to_string()],
                kind: ViewKind::Schedule,
            },
        ];

        // A controls view with a light button, restricted to family.
        views.push(ViewConfig {
            id: "lounge".to_string(),
            title: "Lounge".to_string(),
            menu: None,
            groups: vec!["family".to_string()],
            kind: ViewKind::Controls(ControlsConfig {
                rows: vec![robotica_common::config::ButtonRowConfig {
                    id: "lights".to_string(),
                    title: "Lights".to_string(),
                    buttons: vec![robotica_common::config::ButtonConfig {
                        id: "light".to_string(),
                        title: "Light".to_string(),
                        icon: robotica_common::config::Icon::Light,
                        controller: robotica_common::config::ControllerConfig::Light(
                            robotica_common::controllers::robotica::lights::Config {
                                topic_substr: "lounge/light".to_string(),
                                action: robotica_common::controllers::Action::Toggle,
                                scene: robotica_common::robotica::lights::SceneName::new("on"),
                            },
                        ),
                    }],
                }],
            }),
        });

        Arc::new(UiConfig {
            instance: "home".to_string(),
            views,
        })
    }

    #[test]
    fn test_user_topics_guest() {
        let config = ui_config();
        let topics = user_topics(&user(&[]), Some(&config));

        // The guest sees only the tags view topics.
        assert!(topics.is_subscribe_allowed("robotica/home/tags"));
        assert!(!topics.is_subscribe_allowed("schedule/home/pending"));
        assert!(!topics.is_subscribe_allowed("robotica/state/lounge/light/power"));
        assert!(!topics.is_send_allowed("robotica/command/lounge/light"));
    }

    #[test]
    fn test_user_topics_family() {
        let config = ui_config();
        let topics = user_topics(&user(&["family"]), Some(&config));

        // The family member sees all views' topics.
        assert!(topics.is_subscribe_allowed("robotica/home/tags"));
        assert!(topics.is_subscribe_allowed("schedule/home/pending"));
        assert!(topics.is_subscribe_allowed("robotica/state/lounge/light/power"));
        assert!(topics.is_subscribe_allowed("robotica/state/lounge/light/scene"));
        assert!(topics.is_send_allowed("robotica/command/lounge/light"));
    }

    #[test]
    fn test_user_topics_fails_closed_without_config() {
        let topics = user_topics(&user(&["family"]), None);
        assert!(!topics.is_subscribe_allowed("robotica/home/tags"));
        assert!(!topics.is_send_allowed("cmnd/anything/POWER"));
    }

    #[test]
    fn test_checks() {
        let mut topics = Topics::default();
        topics.subscribe_exact("stat/fan/POWER".to_string());
        topics.send_exact("cmnd/fan/POWER".to_string());

        assert!(check_topic_subscribe_allowed("stat/fan/POWER", &topics));
        assert!(!check_topic_subscribe_allowed("stat/other/POWER", &topics));
        assert!(check_topic_send_allowed("cmnd/fan/POWER", &topics));
        assert!(!check_topic_send_allowed("cmnd/other/POWER", &topics));
    }
}
