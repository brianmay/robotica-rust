//! HTTP server
mod oidc;
mod urls;
mod websocket;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
use robotica_common::version;
use serde::de::Error;
use serde::Deserialize;
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
#[derive(Deserialize)]
pub struct Config {
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

#[derive(Debug, Error)]
enum ManifestLoadError {
    #[error("failed to load manifest.json")]
    LoadError(#[from] std::io::Error),

    #[error("failed to parse manifest.json")]
    ParseError(#[from] serde_json::Error),
}

#[derive(Deserialize)]
struct Manifest(HashMap<String, String>);

impl Manifest {
    async fn load(static_path: &Path) -> Result<Self, ManifestLoadError> {
        let manifest_path = static_path.join("manifest.json");
        let manifest_str = fs::read_to_string(manifest_path).await?;
        let manifest: Self = serde_json::from_str(&manifest_str)?;
        Ok(manifest)
    }

    async fn load_or_default(static_path: &Path) -> Self {
        Self::load(static_path).await.unwrap_or_else(|err| {
            tracing::error!("failed to load manifest: {}", err);
            Self(HashMap::new())
        })
    }

    fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.0.get(key).map_or(key, |s| s.as_str())
    }
}

#[derive(Clone)]
struct HttpState {
    mqtt: MqttTx,
    config: Arc<Config>,
    oidc_client: Arc<ArcSwap<Client>>,
    rooms: Arc<Rooms>,
    manifest: Arc<Manifest>,
}

impl FromRef<HttpState> for MqttTx {
    fn from_ref(state: &HttpState) -> Self {
        state.mqtt.clone()
    }
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

impl FromRef<HttpState> for Arc<Manifest> {
    fn from_ref(state: &HttpState) -> Self {
        state.manifest.clone()
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
pub async fn run(mqtt: MqttTx, rooms: Rooms, config: Config) -> Result<(), HttpError> {
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
    let oidc_client = Arc::new(ArcSwap::new(Arc::new(client)));

    let config = Arc::new(config);
    let rooms = Arc::new(rooms);
    let manifest = Arc::new(Manifest::load_or_default(&config.static_path).await);

    {
        let client = oidc_client.clone();
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

    let state = HttpState {
        mqtt,
        config,
        oidc_client,
        rooms,
        manifest,
    };

    spawn(async {
        server(state, session_layer).await.unwrap_or_else(|err| {
            error!("http server failed: {}", err);
        });
    });

    Ok(())
}

async fn server(
    state: HttpState,
    session_layer: SessionLayer<CookieStore>,
) -> Result<(), HttpError> {
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

const ASSET_SUFFIXES: [&str; 9] = [
    ".js", ".css", ".png", ".jpg", ".jpeg", ".svg", ".ico", ".woff2", "*.json",
];

async fn fallback_handler(
    session: ReadableSession,
    State(oidc_client): State<Arc<Client>>,
    State(http_config): State<Arc<Config>>,
    State(manifest): State<Arc<Manifest>>,
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
                StatusCode::NOT_FOUND if !asset_file => serve_index_html(&manifest).await,
                _ => response.map(boxed),
            }
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("error: {err}")).into_response(),
    }
}

async fn serve_index_html(manifest: &Manifest) -> Response {
    let index_path = manifest.get("/index.html");
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
async fn root(session: ReadableSession, State(manifest): State<Arc<Manifest>>) -> Response {
    let version = version::Version::get();

    let user = get_user(&session);
    let backend_js = manifest.get("/backend.js");

    Html(
        html!(
        (DOCTYPE)
        html {
            head {
                title { "Robotica" }
                meta name="viewport" content="width=device-width, initial-scale=1, shrink-to-fit=no" {}
                script src=(backend_js) {}
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
                        div { (format!("Build Date: {}", version.build_date)) }
                        div { (format!("Version: {}", version.vcs_ref)) }
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
