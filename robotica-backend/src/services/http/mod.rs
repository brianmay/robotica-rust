//! HTTP server
mod oidc;
mod urls;
mod websocket;

use std::path::PathBuf;
use std::sync::Arc;
use std::{collections::HashMap, env};

use arc_swap::ArcSwap;
use axum::body::{boxed, Body};
use axum::extract::{FromRef, State};
use axum::http::uri::PathAndQuery;
use axum::http::Request;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Json;
use axum::{extract::Query, routing::get, Router};
use axum_sessions::async_session::CookieStore;
use axum_sessions::extractors::ReadableSession;
use axum_sessions::extractors::WritableSession;
use axum_sessions::{SameSite, SessionLayer};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use maud::{html, Markup, DOCTYPE};
use reqwest::{Method, StatusCode};
use robotica_common::config::Rooms;
use serde::de::Error;
use thiserror::Error;
use tokio::fs;
use tower::{ServiceBuilder, ServiceExt};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::error;

use robotica_common::user::User;

use crate::services::http::websocket::websocket_handler;
use crate::services::mqtt::MqttTx;
use crate::spawn;

use self::oidc::Client;

/// The configuration for the HTTP service.
#[derive(Clone)]
pub struct Config {
    /// The MQTT client.
    pub mqtt: MqttTx,

    /// The root URL for the HTTP service.
    pub root_url: reqwest::Url,

    /// The path to the static files.
    pub static_path: PathBuf,

    /// The secret used to sign the session cookie.
    pub session_secret: String,

    /// The OIDC discovery URL.
    pub oidc_discovery_url: String,

    /// The OIDC client ID.
    pub oidc_client_id: String,

    /// The OIDC client secret.
    pub oidc_client_secret: String,

    /// The OIDC scopes.
    pub oidc_scopes: String,

    /// The HTTP listener address.
    pub http_listener: String,
}

impl Config {
    // fn generate_url(&self, path: &str) -> Result<String, url::ParseError> {
    //     urls::generate_url(&self.root_url, path)
    // }

    fn generate_url_or_default(&self, path: &str) -> String {
        urls::generate_url_or_default(&self.root_url, path)
    }
}

#[derive(Clone)]
struct HttpState {
    config: Arc<Config>,
    oidc_client: Arc<ArcSwap<Client>>,
    rooms: Arc<Rooms>,
}

impl FromRef<HttpState> for Arc<Config> {
    fn from_ref(state: &HttpState) -> Self {
        state.config.clone()
    }
}

impl FromRef<HttpState> for Arc<Client> {
    fn from_ref(state: &HttpState) -> Self {
        let x = state.oidc_client.load();
        x.clone()
    }
}

impl FromRef<HttpState> for Arc<Rooms> {
    fn from_ref(state: &HttpState) -> Self {
        state.rooms.clone()
    }
}

/// An error running the HTTP service.
#[derive(Error, Debug)]
pub enum HttpError {
    /// There was an error configuring OIDC support.
    #[error("OIDC error: {0}")]
    Oidc(#[from] oidc::Error),

    /// There was an error decoding the base64 secret.
    #[error("Base64 Decode Error")]
    Base64Decode(#[from] base64::DecodeError),

    /// URL Parse error
    #[error("URL Parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    /// Address parse error
    #[error("Address parse error: {0}")]
    AddrParse(#[from] std::net::AddrParseError),

    /// Hyper error
    #[error("Hyper error: {0}")]
    Hyper(#[from] hyper::Error),
}

/// Run the HTTP service.
///
/// # Errors
///
/// This function will return an error if there is a problem configuring the HTTP service.
#[allow(clippy::unused_async)]
pub async fn run(rooms: Rooms, config: Config) -> Result<(), HttpError> {
    let store = CookieStore::new();
    let secret = STANDARD.decode(config.session_secret.clone())?;
    let session_layer = SessionLayer::new(store, &secret).with_same_site_policy(SameSite::Lax);

    let redirect = config
        .generate_url_or_default("/openid_connect_redirect_uri?iss=https://auth.linuxpenguins.xyz");

    let oidc_config = oidc::Config {
        issuer: config.oidc_discovery_url.clone(),
        client_id: config.oidc_client_id.clone(),
        client_secret: config.oidc_client_secret.clone(),
        redirect_uri: redirect,
        scopes: config.oidc_scopes.clone(),
    };

    let client = Client::new(&oidc_config).await?;
    let client = Arc::new(ArcSwap::new(Arc::new(client)));

    let config = Arc::new(config);
    let rooms = Arc::new(rooms);

    {
        let client = client.clone();
        spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60 * 60)).await;

                tracing::info!("refreshing oidc client");
                let new_client = Client::new(&oidc_config).await;
                match new_client {
                    Ok(new_client) => {
                        client.store(Arc::new(new_client));
                    }
                    Err(e) => {
                        tracing::error!("failed to refresh oidc client: {}", e);
                    }
                }
            }
        });
    }

    spawn(async {
        server(config, client, rooms, session_layer)
            .await
            .unwrap_or_else(|err| {
                error!("http server failed: {}", err);
            });
    });

    Ok(())
}

async fn server(
    config: Arc<Config>,
    oidc_client: Arc<ArcSwap<Client>>,
    rooms: Arc<Rooms>,
    session_layer: SessionLayer<CookieStore>,
) -> Result<(), HttpError> {
    let state = HttpState {
        config,
        oidc_client,
        rooms,
    };

    let http_listener = state.config.http_listener.clone();

    let app = Router::new()
        .route("/", get(root))
        .route("/openid_connect_redirect_uri", get(oidc_callback))
        .route("/websocket", get(websocket_handler))
        .route("/rooms", get(rooms_handler))
        .fallback(fallback_handler)
        .with_state(state)
        .layer(session_layer)
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    // let c = (*config).http_listener;
    let addr = http_listener.parse()?;
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

fn set_user(
    session: &mut WritableSession,
    user_info: &openid::Userinfo,
) -> Result<(), serde_json::Error> {
    let closure = || {
        let sub = user_info.sub.clone()?;
        let name = user_info.name.clone()?;
        let email = user_info.email.clone()?;
        let user = User { sub, name, email };
        Some(user)
    };

    let user = closure().ok_or_else(|| serde_json::Error::custom("Missing user info"))?;
    session.insert("user", &user)
}

fn get_user(session: &ReadableSession) -> Option<User> {
    session.get::<User>("user")
}

const ASSET_SUFFIXES: [&str; 8] = [
    ".js", ".css", ".png", ".jpg", ".jpeg", ".svg", ".ico", ".woff2",
];

async fn fallback_handler(
    session: ReadableSession,
    State(oidc_client): State<Arc<Client>>,
    State(http_config): State<Arc<Config>>,
    req: Request<Body>,
) -> Response {
    if req.method() != Method::GET {
        return (StatusCode::METHOD_NOT_ALLOWED, "Method not allowed").into_response();
    }

    let asset_file = {
        let path = req.uri().path();
        ASSET_SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
    };

    if !asset_file && get_user(&session).is_none() {
        let origin_url = req.uri().path_and_query().map_or("/", PathAndQuery::as_str);
        let auth_url = oidc_client.get_auth_url(origin_url);
        return Redirect::to(&auth_url).into_response();
    }

    let static_path = &http_config.static_path;
    match ServeDir::new(static_path).oneshot(req).await {
        Ok(response) => {
            let status = response.status();
            match status {
                // If this is an asset file, then don't redirect to index.html
                StatusCode::NOT_FOUND if !asset_file => serve_index_html(static_path).await,
                _ => response.map(boxed),
            }
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("error: {err}")).into_response(),
    }
}

async fn serve_index_html(static_path: &PathBuf) -> Response {
    let index_path = PathBuf::from(static_path).join("index.html");
    {
        let this = fs::read_to_string(index_path)
            .await
            .map(|index_content| (StatusCode::OK, Html(index_content)).into_response());
        this.map_or_else(
            |_| (StatusCode::INTERNAL_SERVER_ERROR, "index.html not found").into_response(),
            |t| t,
        )
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
                            a class="nav-link" href="/welcome" { "Login" }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::unused_async)]
async fn root(session: ReadableSession) -> Response {
    let build_date = env::var("BUILD_DATE").unwrap_or_else(|_| "unknown".to_string());
    let vcs_ref = env::var("VCS_REF").unwrap_or_else(|_| "unknown".to_string());

    let user = get_user(&session);

    Html(
        html!(
        (DOCTYPE)
        html {
            head {
                title { "Robotica" }
                meta name="viewport" content="width=device-width, initial-scale=1, shrink-to-fit=no" {}
                script src="backend.js" {}
            }
            body {
                ( nav_bar() )
                h1 { "Robotica" }
                p {
                    @match user {
                        Some(user) => ( format!("Hello, {user}!") ),
                        None => ( "You are not logged in!" ),
                    }
                }
                footer {
                    div {
                        div { (format!("Build Date: {build_date}")) }
                        div { (format!("Version: {vcs_ref}")) }
                    }
                    div {
                        "Robotica"
                    }
                }
            }
        }
    ).into_string()).into_response()
}

async fn oidc_callback(
    State(http_config): State<Arc<Config>>,
    State(oidc_client): State<Arc<Client>>,
    Query(params): Query<HashMap<String, String>>,
    mut session: WritableSession,
) -> Response {
    let code = params.get("code").cloned().unwrap_or_default();

    let state = params
        .get("state")
        .cloned()
        .unwrap_or_else(|| "/".to_string());

    let result = oidc_client.request_token(&code).await;

    match result {
        Ok((_token, user_info)) => {
            set_user(&mut session, &user_info).unwrap_or_else(|err| {
                tracing::error!("failed to set user in session: {err}");
            });

            let url = http_config.generate_url_or_default(&state);
            Redirect::to(&url).into_response()
        }
        Err(e) => {
            session.destroy();
            Html(
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
                .into_string(),
            )
            .into_response()
        }
    }
}

#[allow(clippy::unused_async)]
async fn rooms_handler(State(rooms): State<Arc<Rooms>>, session: ReadableSession) -> Json<Rooms> {
    let rooms = if get_user(&session).is_some() {
        rooms
    } else {
        Arc::new(Rooms::default())
    };
    Json((*rooms).clone())
}
