use axum::response::{IntoResponse, Response};
use hyper::StatusCode;
use maud::{html, DOCTYPE};
use tap::Pipe;
use thiserror::Error;
use tracing::error;

use crate::services::http::{footer, nav_bar};

#[derive(Debug, Error)]
pub enum ResponseError {
    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Method not allowed")]
    MethodNotAllowed,

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("SQL error: {0}")]
    SqlError(#[from] sqlx::Error),

    #[error("OIDC error")]
    OidcError(),
}

impl ResponseError {
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::InternalError(message.into())
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }
    pub const fn sql_error(err: sqlx::Error) -> Self {
        Self::SqlError(err)
    }
}

impl IntoResponse for ResponseError {
    fn into_response(self) -> Response {
        match self {
            Self::AuthenticationFailed => {
                error_page(StatusCode::UNAUTHORIZED, "Authentication failed")
            }
            Self::MethodNotAllowed => error_page(StatusCode::METHOD_NOT_ALLOWED, "Invalid method"),
            Self::InternalError(message) => {
                error!("Internal error: {}", message);
                error_page(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
            }
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message).into_response(),
            Self::OidcError() => (StatusCode::INTERNAL_SERVER_ERROR, "OIDC Error").into_response(),
            Self::SqlError(err) => {
                error!("SQL Error: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, "SQL Error").into_response()
            }
        }
    }
}

fn error_page(status: StatusCode, message: &str) -> Response {
    let message = status.canonical_reason().map_or_else(
        || format!("{status} {message}"),
        |reason| format!("{status} {reason} {message}"),
    );

    let body = html!(
        (DOCTYPE)
        html {
            head {
                title { "Robotica - Error" }
                meta name="viewport" content="width=device-width, initial-scale=1, shrink-to-fit=no" {}
            }
            body {
                ( nav_bar() )
                h1 { "Robotica - Error" (status) }
                p {
                    (message)
                }
                (footer() )
            }
        };
    ).pipe(axum_core::response::IntoResponse::into_response);

    (status, body).into_response()
}
