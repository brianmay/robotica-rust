use axum_core::response::{IntoResponse, Response};
use hyper::StatusCode;
use tracing::error;

pub enum ResponseError {
    AuthenticationFailed,
    MethodNotAllowed,
    InternalError(String),
    BadRequest(String),
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
        }
    }
}