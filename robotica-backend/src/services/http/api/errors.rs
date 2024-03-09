use axum::Json;
use axum_core::response::{IntoResponse, Response};
use hyper::StatusCode;
use robotica_common::robotica::http_api::api_error;
use thiserror::Error;
use tracing::error;

#[derive(Debug, Error)]
pub enum ResponseError {
    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("SQL error: {0}")]
    SqlError(#[from] sqlx::Error),

    #[error("Object does not exist")]
    NotFoundError(),
}

impl ResponseError {
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::InternalError(message.into())
    }
}

impl IntoResponse for ResponseError {
    fn into_response(self) -> Response {
        match self {
            Self::AuthenticationFailed => {
                let error = api_error("Authentication failed");
                (StatusCode::UNAUTHORIZED, Json(error)).into_response()
            }
            Self::InternalError(message) => {
                error!("Internal error: {}", message);
                let error = api_error("Internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
            }
            Self::SqlError(err) => {
                error!("SQL Error: {}", err);
                let error = api_error("SQL Error");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
            }
            Self::NotFoundError() => {
                let error = api_error("Not Found");
                (StatusCode::NOT_FOUND, Json(error)).into_response()
            }
        }
    }
}
