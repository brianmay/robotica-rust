mod oidc;
mod urls;

use std::include_str;
use std::str::Utf8Error;
use std::sync::Arc;
use std::{collections::HashMap, env};

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::WebSocketUpgrade;
use axum::http::Uri;
use axum::response::{Html, IntoResponse, Response};
use axum::{extract::Query, routing::get, Extension, Router};
use axum_sessions::extractors::ReadableSession;
use axum_sessions::{async_session::MemoryStore, extractors::WritableSession};
use axum_sessions::{SameSite, SessionLayer};
use base64::decode;
use futures::{sink::SinkExt, stream::StreamExt};
use maud::{html, Markup};
use reqwest::StatusCode;
use robotica_rust::sources::mqtt;
use robotica_rust::{entities::Sender, get_env, sources::mqtt::Mqtt, spawn, EnvironmentError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::select;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::State;

use self::oidc::Client;
use self::urls::generate_url_or_default;

pub(crate) struct HttpState {
    #[allow(dead_code)]
    mqtt: Mqtt,
    #[allow(dead_code)]
    message_sink: Sender<String>,
    root_url: reqwest::Url,
}

#[derive(Error, Debug)]
pub enum HttpError {
    #[error("Environment error: {0}")]
    Environment(#[from] EnvironmentError),

    #[error("OIDC error: {0}")]
    Oidc(#[from] oidc::Error),

    #[error("Base64 Decode Error")]
    Base64Decode(#[from] base64::DecodeError),

    // Parse error
    #[error("URL Parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

pub async fn run(state: &mut State) -> Result<(), HttpError> {
    let http_state = HttpState {
        mqtt: state.mqtt.clone(),
        message_sink: state.message_sink.clone(),
        root_url: reqwest::Url::parse(&get_env("ROOT_URL")?)?,
    };

    let store = MemoryStore::new();
    let secret = decode(get_env("SESSION_SECRET")?)?;
    let session_layer = SessionLayer::new(store, &secret).with_same_site_policy(SameSite::Lax);

    let redirect = generate_url_or_default(
        &http_state,
        "/openid_connect_redirect_uri?iss=https://auth.linuxpenguins.xyz",
    );

    let config = oidc::Config {
        issuer: get_env("OIDC_DISCOVERY_URL")?,
        client_id: get_env("OIDC_CLIENT_ID")?,
        client_secret: get_env("OIDC_CLIENT_SECRET")?,
        redirect_uri: redirect,
        scopes: vec!["openid".to_string(), "profile".to_string()],
    };

    let client = Client::new(config).await?;

    spawn(async {
        server(http_state, client, session_layer)
            .await
            .expect("http server failed");
    });

    Ok(())
}

async fn server(
    http_state: HttpState,
    oidc: Client,
    session_layer: SessionLayer<MemoryStore>,
) -> Result<(), HttpError> {
    let http_state = Arc::new(http_state);
    let oidc = Arc::new(oidc);

    let app = Router::new()
        .route("/", get(root))
        .route("/test", get(test))
        .route("/login", get(login))
        .route("/openid_connect_redirect_uri", get(oidc_callback))
        .route("/websocket", get(websocket_handler))
        .layer(Extension(http_state))
        .layer(Extension(oidc))
        .layer(session_layer);

    let addr = "[::]:4000".parse().unwrap();
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn root(state: Extension<Arc<HttpState>>, session: ReadableSession) -> Markup {
    let build_date = env::var("BUILD_DATE").unwrap_or_else(|_| "unknown".to_string());
    let vcs_ref = env::var("VCS_REF").unwrap_or_else(|_| "unknown".to_string());

    let user = session.get::<String>("user");

    let login_url = generate_url_or_default(&state, "/login");

    html!(
        html {
            head {
                title { "Robotica" }
            }
            body {
                h1 { "Robotica" }
                p { "You are boring. Go away." }
                p { (format!("{:?}", user)) }
                p { "Build date: " (build_date) }
                p { "VCS ref: " (vcs_ref) }
                a href=(login_url) { "Login" }
            }
        }
    )
}

// Include utf-8 file at **compile** time.
async fn test() -> Html<&'static str> {
    Html(include_str!("../chat.html"))
}

async fn login(
    state: Extension<Arc<HttpState>>,
    oidc_client: Extension<Arc<Client>>,
    origin_url: Uri,
) -> Markup {
    let origin_url = origin_url.path_and_query().unwrap().as_str();
    let auth_url = oidc_client.get_auth_url(origin_url).unwrap();
    let root_url = generate_url_or_default(&state, "/");

    html!(
        html {
            head {
                title { "Robotica - Login" }
            }
            body {
                h1 { "Login" }
                p { "Do I know you?" }
                p { "Links"
                    ul {
                        li { a href=(root_url) { "Home" } }
                        li { a href=(auth_url) { "Login" } }
                    }
                }
            }
        }
    )
}

async fn oidc_callback(
    state: Extension<Arc<HttpState>>,
    oidc_client: Extension<Arc<Client>>,
    Query(params): Query<HashMap<String, String>>,
    mut session: WritableSession,
) -> Markup {
    let root_url = generate_url_or_default(&state, "/");

    let code = params
        .get("code")
        .cloned()
        .unwrap_or_else(|| "".to_string());

    let result = oidc_client.request_token(&code).await;

    let user = match result {
        Ok((_token, user_info)) => {
            session.insert("user", &user_info.name).unwrap();
            Ok(format!("{:?}", user_info))
        }
        Err(e) => {
            session.destroy();
            Err(format!("{}", e))
        }
    };

    html!(
        html {
            head {
                title { "Robotica - Login" }
            }
            body {
                h1 { "Login" }
                @match user {
                    Ok(user) => {
                        p { "YES! I know you!" }
                        p { (user) }
                    }
                    Err(err) => {
                        p { "NO! I don't know you!" }
                        p { (err) }
                    }
                }
                p {
                    a href=(root_url) { "Home" }
                }
            }
        }
    )
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<Arc<HttpState>>,
    session: ReadableSession,
) -> Response {
    if let Some(_name) = session.get::<String>("user") {
        ws.on_upgrade(|socket| websocket(socket, state))
            .into_response()
    } else {
        Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::empty())
            .unwrap()
            .into_response()
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum WsMessage {
    Subscribe { topic: String },
    Send(MqttMessage),
}

#[derive(Debug, Serialize, Deserialize)]
struct MqttMessage {
    topic: String,
    payload: String,
}

impl From<MqttMessage> for mqtt::Message {
    fn from(msg: MqttMessage) -> Self {
        mqtt::Message::from_string(&msg.topic, &msg.payload, false, mqtt::QoS::exactly_once())
    }
}

impl TryFrom<mqtt::Message> for MqttMessage {
    type Error = Utf8Error;

    fn try_from(msg: mqtt::Message) -> Result<Self, Self::Error> {
        let payload = msg.payload_into_string()?;
        Ok(MqttMessage {
            topic: msg.topic,
            payload,
        })
    }
}

async fn websocket(stream: WebSocket, state: Arc<HttpState>) {
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
                    let msg: Result<WsMessage, _> = serde_json::from_str(&msg);
                    match msg {
                        Ok(WsMessage::Subscribe { topic }) => {
                            process_subscribe(topic, &state, tx.clone()).await;
                        }
                        Ok(WsMessage::Send(msg)) => {
                            tracing::info!("recv_task: Sending message to mqtt {}: {}", msg.topic, msg.payload);
                            let message: mqtt::Message = msg.into();
                            state.mqtt.try_send(message);
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

        debug!("ending recv_task");
    });

    let _ = recv_task.await;
    send_task.abort();
}

async fn process_subscribe(
    topic: String,
    state: &Arc<HttpState>,
    sender: mpsc::UnboundedSender<MqttMessage>,
) {
    info!("Subscribing to {}", topic);
    let rc = state.mqtt.subscribe(&topic).await;
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
