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
use axum::{extract::Query, routing::get, Router};
use axum_sessions::async_session::CookieStore;
use axum_sessions::extractors::ReadableSession;
use axum_sessions::extractors::WritableSession;
use axum_sessions::{SameSite, SessionLayer};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use hyper::header::CONTENT_TYPE;
use maud::{html, Markup, DOCTYPE};
use reqwest::{Method, StatusCode};
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
use crate::{get_env, spawn, EnvironmentError};

use self::oidc::Client;

#[derive(Clone)]
struct HttpConfig {
    #[allow(dead_code)]
    mqtt: MqttTx,
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

#[derive(Clone)]
struct HttpState {
    http_config: HttpConfig,
    oidc_client: Arc<ArcSwap<Client>>,
}

impl FromRef<HttpState> for HttpConfig {
    fn from_ref(state: &HttpState) -> Self {
        state.http_config.clone()
    }
}

impl FromRef<HttpState> for Arc<Client> {
    fn from_ref(state: &HttpState) -> Self {
        let x = state.oidc_client.load();
        x.clone()
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
pub async fn run(mqtt: MqttTx) -> Result<(), HttpError> {
    let http_config = HttpConfig {
        mqtt,
        root_url: reqwest::Url::parse(&get_env("ROOT_URL")?)?,
    };

    let store = CookieStore::new();
    let secret = STANDARD.decode(get_env("SESSION_SECRET")?)?;
    let session_layer = SessionLayer::new(store, &secret).with_same_site_policy(SameSite::Lax);

    let redirect = http_config
        .generate_url_or_default("/openid_connect_redirect_uri?iss=https://auth.linuxpenguins.xyz");

    let config = oidc::Config {
        issuer: get_env("OIDC_DISCOVERY_URL")?,
        client_id: get_env("OIDC_CLIENT_ID")?,
        client_secret: get_env("OIDC_CLIENT_SECRET")?,
        redirect_uri: redirect,
        scopes: get_env("OIDC_SCOPES")?,
    };

    let client = Client::new(config.clone()).await?;
    let client = Arc::new(ArcSwap::new(Arc::new(client)));

    {
        let client = client.clone();
        spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60 * 60)).await;

                tracing::info!("refreshing oidc client");
                let new_client = Client::new(config.clone()).await;
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
        server(http_config, client, session_layer)
            .await
            .unwrap_or_else(|err| {
                error!("http server failed: {}", err);
            });
    });

    Ok(())
}

async fn server(
    http_config: HttpConfig,
    oidc_client: Arc<ArcSwap<Client>>,
    session_layer: SessionLayer<CookieStore>,
) -> Result<(), HttpError> {
    let state = HttpState {
        http_config,
        oidc_client,
    };

    let app = Router::new()
        .route("/", get(root))
        .route("/openid_connect_redirect_uri", get(oidc_callback))
        .route("/websocket", get(websocket_handler))
        .fallback(fallback_handler)
        .with_state(state)
        .layer(session_layer)
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    let http_listener = get_env("HTTP_LISTENER").unwrap_or_else(|_| "[::]:4000".to_string());
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

    let static_path = "./brian-frontend/dist";
    match ServeDir::new(static_path).oneshot(req).await {
        Ok(response) => {
            let status = response.status();
            match status {
                // If this is an asset file, then don't redirect to index.html
                StatusCode::NOT_FOUND if !asset_file => {
                    let index_path = PathBuf::from(static_path).join("index.html");
                    let index_content = match fs::read_to_string(index_path).await {
                        Err(_) => {
                            return (StatusCode::INTERNAL_SERVER_ERROR, "index file not found")
                                .into_response();
                        }
                        Ok(index_content) => index_content,
                    };

                    (StatusCode::OK, [(CONTENT_TYPE, "text/html")], index_content).into_response()
                }
                _ => response.map(boxed),
            }
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("error: {err}")).into_response(),
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
    State(http_config): State<HttpConfig>,
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
