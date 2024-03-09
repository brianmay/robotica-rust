//! HTTP server
mod api;
mod errors;
mod oidc;
mod urls;
mod websocket;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::body::Body;
use axum::extract::{FromRef, State};
use axum::http::uri::PathAndQuery;
use axum::http::Request;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::{extract::Query, routing::get, Router};
use hyper::{Method, StatusCode};
use maud::{html, Markup, DOCTYPE};
use robotica_common::config as ui_config;
use robotica_common::version;
use serde::Deserialize;
use thiserror::Error;
use time::Duration;
use tokio::fs;
use tokio::net::TcpListener;
use tower::{ServiceBuilder, ServiceExt};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tower_sessions::cookie::SameSite;
use tower_sessions::session_store::ExpiredDeletion;
use tower_sessions::{Expiry, Session, SessionManagerLayer};
use tower_sessions_sqlx_store::PostgresStore;
use tracing::error;

use robotica_common::user::User;

use crate::services::http::websocket::websocket_handler;
use crate::services::mqtt::MqttTx;
use crate::spawn;

use self::api::config::config_handler;
use self::api::locations;
use self::errors::ResponseError;
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

    /// The server hostname in MQTT topics.
    pub instance: String,
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
struct Manifest(HashMap<String, String>, PathBuf);

impl Manifest {
    async fn load(static_path: &Path) -> Result<Self, ManifestLoadError> {
        let manifest_path = static_path.join("manifest.json");
        let manifest_str = fs::read_to_string(manifest_path).await?;
        let manifest: HashMap<String, String> = serde_json::from_str(&manifest_str)?;
        Ok(Self(manifest, static_path.to_owned()))
    }

    async fn load_or_default(static_path: &Path) -> Self {
        Self::load(static_path).await.unwrap_or_else(|err| {
            tracing::error!("failed to load manifest: {}", err);
            Self(HashMap::new(), static_path.to_owned())
        })
    }

    fn get_internal<'a>(&'a self, key: &'a str) -> &'a str {
        self.0.get(key).map_or_else(
            || {
                error!("Cannot find {key} in manifest.json");
                key
            },
            |s| s.as_str(),
        )
    }

    fn get_path(&self, key: &str) -> PathBuf {
        self.1.join(self.get_internal(key))
    }

    fn get_url(&self, key: &str) -> String {
        format!("/{}", self.get_internal(key))
    }
}

#[derive(Clone, FromRef)]
struct HttpState {
    mqtt: MqttTx,
    config: Arc<Config>,
    oidc_client: Arc<ArcSwap<Option<Client>>>,
    rooms: Arc<ui_config::Rooms>,
    manifest: Arc<Manifest>,
    postgres: sqlx::PgPool,
}

/// An error running the HTTP service.
#[derive(Error, Debug)]
pub enum HttpError {
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Run the HTTP service.
///
/// # Errors
///
/// This function will return an error if there is a problem configuring the HTTP service.
#[allow(clippy::unused_async)]
pub async fn run(
    mqtt: MqttTx,
    rooms: ui_config::Rooms,
    config: Config,
    postgres: sqlx::PgPool,
) -> Result<(), HttpError> {
    let session_store = PostgresStore::new(postgres.clone());

    tokio::task::spawn(
        session_store
            .clone()
            .continuously_delete_expired(tokio::time::Duration::from_secs(60)),
    );

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)))
        .with_same_site(SameSite::Lax);

    let redirect = config.generate_url_or_default("/openid_connect_redirect_uri");

    let oidc_config = oidc::Config {
        issuer: config.oidc_discovery_url.clone(),
        client_id: config.oidc_client_id.clone(),
        client_secret: config.oidc_client_secret.clone(),
        redirect_uri: redirect,
        scopes: config.oidc_scopes.clone(),
    };

    let oidc_client = Arc::new(ArcSwap::new(Arc::new(None)));

    let config = Arc::new(config);
    let rooms = Arc::new(rooms);
    let manifest = Arc::new(Manifest::load_or_default(&config.static_path).await);

    {
        let client = oidc_client.clone();
        spawn(async move {
            loop {
                tracing::info!("refreshing oidc client");
                let new_client = Client::new(&oidc_config).await;
                match new_client {
                    Ok(new_client) => {
                        client.store(Arc::new(Some(new_client)));
                        tokio::time::sleep(tokio::time::Duration::from_secs(60 * 60)).await;
                    }
                    Err(e) => {
                        tracing::error!("failed to refresh oidc client: {}", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
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
        postgres,
    };

    let http_listener = state.config.http_listener.clone();

    let app = Router::new()
        .route("/", get(root))
        .route("/openid_connect_redirect_uri", get(oidc_callback))
        .route("/websocket", get(websocket_handler))
        .route("/config", get(config_handler))
        .route("/logout", get(logout_handler))
        .fallback(fallback_handler)
        .with_state(state.clone())
        .nest("/api/locations", locations::router(state))
        .layer(session_layer)
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    #[allow(clippy::unwrap_used)]
    spawn(async move {
        server(http_listener, app).await.unwrap_or_else(|err| {
            tracing::error!("failed to start http server: {}", err);
        });
    });

    Ok(())
}

async fn server(http_listener: String, app: Router) -> Result<(), HttpError> {
    let listener = TcpListener::bind(&http_listener).await?;
    tracing::info!("listening on {:?}", listener.local_addr());
    axum::serve(listener, app).await?;
    Ok(())
}

async fn set_user(session: &Session, user_info: &openid::Userinfo) -> Result<(), ResponseError> {
    let closure = || {
        let sub = user_info.sub.clone()?;
        let name = user_info.name.clone()?;
        let email = user_info.email.clone()?;
        let user = User { sub, name, email };
        Some(user)
    };

    let user = closure().ok_or_else(|| ResponseError::bad_request("Missing user info"))?;

    session
        .insert("user", user)
        .await
        .map_err(|err| ResponseError::internal_error(format!("Failed to insert user: {err}")))?;

    Ok(())
}

async fn get_user(session: &Session) -> Option<User> {
    let user = session.get::<User>("user");
    user.await.unwrap_or_default()
}

const ASSET_SUFFIXES: [&str; 9] = [
    ".js", ".css", ".png", ".jpg", ".jpeg", ".svg", ".ico", ".woff2", "*.json",
];

async fn fallback_handler(
    session: Session,
    State(oidc_client): State<Arc<ArcSwap<Option<Client>>>>,
    State(http_config): State<Arc<Config>>,
    State(manifest): State<Arc<Manifest>>,
    req: Request<Body>,
) -> Result<Response, ResponseError> {
    if req.method() != Method::GET {
        return Err(ResponseError::MethodNotAllowed);
    }

    let asset_file = {
        let path = req.uri().path();
        ASSET_SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
    };

    if !asset_file && get_user(&session).await.is_none() {
        let origin_url = req.uri().path_and_query().map_or("/", PathAndQuery::as_str);
        let oidc_client = oidc_client.load();
        let Some(oidc_client) = oidc_client.as_ref() else {
            return Err(ResponseError::OidcError());
        };
        let auth_url = oidc_client.get_auth_url(origin_url);
        return Ok(Redirect::to(&auth_url).into_response());
    }

    let static_path = &http_config.static_path;
    match ServeDir::new(static_path).oneshot(req).await {
        Ok(response) => {
            let status = response.status();
            match status {
                // If this is an asset file, then don't redirect to index.html
                StatusCode::NOT_FOUND if !asset_file => serve_index_html(&manifest).await,
                _ => Ok(response.map(Body::new)),
            }
        }
        Err(err) => Err(ResponseError::internal_error(format!("error: {err}"))),
    }
}

async fn serve_index_html(manifest: &Manifest) -> Result<Response, ResponseError> {
    let index_path = manifest.get_path("index.html");
    {
        fs::read_to_string(index_path)
            .await
            .map(|index_content| Html(index_content).into_response())
            .map_err(|_| ResponseError::internal_error("index.html not found"))
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
async fn root(session: Session, State(manifest): State<Arc<Manifest>>) -> Response {
    let version = version::Version::get();

    let user = get_user(&session).await;
    let backend_js = manifest.get_url("backend.js");

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
    State(oidc_client): State<Arc<ArcSwap<Option<Client>>>>,
    Query(params): Query<HashMap<String, String>>,
    session: Session,
) -> Result<Response, ResponseError> {
    let code = params.get("code").cloned().unwrap_or_default();

    let state = params
        .get("state")
        .cloned()
        .unwrap_or_else(|| "/".to_string());

    let result = {
        let oidc_client = oidc_client.load();
        let Some(oidc_client) = oidc_client.as_ref() else {
            return Err(ResponseError::OidcError());
        };

        oidc_client.request_token(&code).await
    };

    let response = match result {
        Ok((_token, user_info)) => {
            set_user(&session, &user_info).await?;

            let url = http_config.generate_url_or_default(&state);
            Redirect::to(&url).into_response()
        }
        Err(e) => {
            _ = session.delete().await;
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
    };

    Ok(response)
}

async fn logout_handler(session: Session) -> Response {
    match session.delete().await {
        Ok(()) => Redirect::to("/").into_response(),
        Err(e) => Html(
            html!(
                    html {
                        head {
                            title { "Robotica - Logout" }
                        }
                        body {
                            h1 { ( format!("Logout Failed: {e}") ) }
                        }
                    }
            )
            .into_string(),
        )
        .into_response(),
    }
}
