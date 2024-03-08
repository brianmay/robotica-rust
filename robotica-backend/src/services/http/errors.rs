use axum_core::response::{IntoResponse, Response};
use hyper::StatusCode;
use thiserror::Error;
use tracing::error;

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
}

impl IntoResponse for ResponseError {
    fn into_response(self) -> Response {
        match self {
            Self::AuthenticationFailed => {
                (StatusCode::UNAUTHORIZED, "Authentication failed").into_response()
            }
            Self::MethodNotAllowed => {
                (StatusCode::METHOD_NOT_ALLOWED, "Invalid method").into_response()
            }
            Self::InternalError(message) => {
                error!("Internal error: {}", message);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
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
