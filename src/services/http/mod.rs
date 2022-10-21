//! HTTP server
mod oidc;
mod urls;

use std::path::PathBuf;
use std::str::Utf8Error;
use std::sync::Arc;
use std::{collections::HashMap, env};

use axum::body::{boxed, Body};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::WebSocketUpgrade;
use axum::http::Request;
use axum::response::{IntoResponse, Response};
use axum::{extract::Query, routing::get, Extension, Router};
use axum_sessions::extractors::ReadableSession;
use axum_sessions::{async_session::MemoryStore, extractors::WritableSession};
use axum_sessions::{SameSite, SessionLayer};
use base64::decode;
use futures::{sink::SinkExt, stream::StreamExt};
use maud::{html, Markup, DOCTYPE};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::{fs, select};
use tower::{ServiceBuilder, ServiceExt};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info};

use crate::services::mqtt;
use crate::services::mqtt::Mqtt;
use crate::{get_env, spawn, EnvironmentError};

use self::oidc::Client;

pub(crate) struct HttpConfig {
    #[allow(dead_code)]
    mqtt: Mqtt,
    root_url: reqwest::Url,
}

impl HttpConfig {
    // fn generate_url(&self, path: &str) -> Result<String, url::ParseError> {
    //     urls::generate_url(&self.root_url, path)
    // }

    fn generate_url_or_default(&self, path: &str) -> String {
        urls::generate_url_or_default(&self.root_url, path)
    }
}

/// An error running the HTTP service.
#[derive(Error, Debug)]
pub enum HttpError {
    /// There was a problem with an environment variable.
    #[error("Environment error: {0}")]
    Environment(#[from] EnvironmentError),

    /// There was an error configuring OIDC support.
    #[error("OIDC error: {0}")]
    Oidc(#[from] oidc::Error),

    /// There was an error decoding the base64 secret.
    #[error("Base64 Decode Error")]
    Base64Decode(#[from] base64::DecodeError),

    /// URL Parse error
    #[error("URL Parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

/// Run the HTTP service.
///
/// # Errors
///
/// This function will return an error if there is a problem configuring the HTTP service.
#[allow(clippy::unused_async)]
pub async fn run(mqtt: Mqtt) -> Result<(), HttpError> {
    let http_config = HttpConfig {
        mqtt,
        root_url: reqwest::Url::parse(&get_env("ROOT_URL")?)?,
    };

    let store = MemoryStore::new();
    let secret = decode(get_env("SESSION_SECRET")?)?;
    let session_layer = SessionLayer::new(store, &secret).with_same_site_policy(SameSite::Lax);

    let redirect = http_config
        .generate_url_or_default("/openid_connect_redirect_uri?iss=https://auth.linuxpenguins.xyz");

    let config = oidc::Config {
        issuer: get_env("OIDC_DISCOVERY_URL")?,
        client_id: get_env("OIDC_CLIENT_ID")?,
        client_secret: get_env("OIDC_CLIENT_SECRET")?,
        redirect_uri: redirect,
        scopes: vec!["openid".to_string(), "profile".to_string()],
    };

    let client = Client::new(config).await?;

    spawn(async {
        server(http_config, client, session_layer)
            .await
            .expect("http server failed");
    });

    Ok(())
}

async fn server(
    config: HttpConfig,
    oidc: Client,
    session_layer: SessionLayer<MemoryStore>,
) -> Result<(), HttpError> {
    let http_state = Arc::new(config);
    let oidc = Arc::new(oidc);

    let app = Router::new()
        .route("/", get(root))
        .route("/openid_connect_redirect_uri", get(oidc_callback))
        .route("/websocket", get(websocket_handler))
        .fallback(get(fallback_handler))
        .layer(Extension(http_state))
        .layer(Extension(oidc))
        .layer(session_layer)
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    let addr = "[::]:4000".parse().unwrap();
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

fn get_user(session: &ReadableSession) -> Option<String> {
    session.get::<String>("user")
}

async fn fallback_handler(
    session: ReadableSession,
    oidc_client: Extension<Arc<Client>>,
    req: Request<Body>,
) -> Response {
    if get_user(&session).is_none() {
        let origin_url = req.uri().path_and_query().unwrap().as_str();
        let auth_url = oidc_client.get_auth_url(origin_url);
        return Response::builder()
            .status(StatusCode::FOUND)
            .header("Location", auth_url)
            .body(Body::empty())
            .unwrap()
            .into_response();
    }

    match ServeDir::new("./dist").oneshot(req).await {
        Ok(response) => {
            let status = response.status();
            match status {
                StatusCode::NOT_FOUND => {
                    let index_path = PathBuf::from("./dist").join("index.html");
                    let index_content = match fs::read_to_string(index_path).await {
                        Err(_) => {
                            return Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(boxed(Body::from("index file not found")))
                                .unwrap()
                        }
                        Ok(index_content) => index_content,
                    };

                    Response::builder()
                        .status(StatusCode::OK)
                        .body(boxed(Body::from(index_content)))
                        .unwrap()
                }
                _ => response.map(boxed),
            }
        }
        Err(err) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(boxed(Body::from(format!("error: {err}"))))
            .expect("error response"),
    }
}

fn nav_bar() -> Markup {
    html! {
        nav class="navbar navbar-expand-sm navbar-dark bg-dark navbar-fixed-top" role="navigation" {
            div class="container-fluid" {
                a class="navbar-brand" href="/" { "Robotica" }
                button class="navbar-toggler" type="button" data-bs-toggle="collapse" data-bs-target="#navbarNav" aria-controls="navbarNav" aria-expanded="false" aria-label="Toggle navigation" {
                    span class="navbar-toggler-icon" {}
                }
                div class="collapse navbar-collapse" id="navbarNav" {
                    div class="navbar-nav" {
                        li class="nav-item" {
                            a class="nav-link" href="/chat" { "Chat" }
                        }
                        li class="nav-item" {
                            a class="nav-link" aria-current="page" href="/login" { "Login" }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::unused_async)]
async fn root(session: ReadableSession) -> Markup {
    let build_date = env::var("BUILD_DATE").unwrap_or_else(|_| "unknown".to_string());
    let vcs_ref = env::var("VCS_REF").unwrap_or_else(|_| "unknown".to_string());

    let user = get_user(&session);

    html!(
        (DOCTYPE)
        html {
            head {
                title { "Robotica" }
                link rel="stylesheet" href="bootstrap.min.css" {}
                script src="bootstrap.min.js" {}
            }
            body {
                ( nav_bar() )
                h1 { "Robotica" }
                p { @match user {
                    Some(user) => ( format!("Hello, {}!", user) ),
                    None => ( "You are not logged in!" ),
                } }
                p { "Build date: " (build_date) ", VCS ref: " (vcs_ref) }
            }
        }
    )
}

async fn oidc_callback(
    http_config: Extension<Arc<HttpConfig>>,
    oidc_client: Extension<Arc<Client>>,
    Query(params): Query<HashMap<String, String>>,
    mut session: WritableSession,
) -> Response {
    let code = params
        .get("code")
        .cloned()
        .unwrap_or_else(|| "".to_string());

    let state = params
        .get("state")
        .cloned()
        .unwrap_or_else(|| "/".to_string());

    let result = oidc_client.request_token(&code).await;

    match result {
        Ok((_token, user_info)) => {
            session.insert("user", &user_info.name).unwrap();

            let url = http_config.generate_url_or_default(&state);

            Response::builder()
                .status(StatusCode::FOUND)
                .header("Location", url)
                .body(Body::empty())
                .unwrap()
                .into_response()
        }
        Err(e) => {
            session.destroy();
            html!(
                    html {
                    head {
                        title { "Robotica - Login" }
                    }
                    body {
                        h1 { ( format!("Login Failed: {e}") ) }
                    }
                }
            )
            .into_response()
        }
    }
}

#[allow(clippy::unused_async)]
async fn websocket_handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<Arc<HttpConfig>>,
    session: ReadableSession,
) -> Response {
    #[allow(clippy::option_if_let_else)]
    if let Some(_name) = get_user(&session) {
        info!("Accessing websocket");
        ws.on_upgrade(|socket| websocket(socket, state))
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

async fn websocket(stream: WebSocket, state: Arc<HttpConfig>) {
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

        debug!("recv_task: ending recv_task");
    });

    let _rc = recv_task.await;
    send_task.abort();
}

async fn process_subscribe(
    topic: String,
    state: &Arc<HttpConfig>,
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
