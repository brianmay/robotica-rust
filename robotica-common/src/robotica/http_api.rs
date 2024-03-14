//! A module for defining the HTTP API.
use std::ops::Deref;

use serde::{Deserialize, Serialize};

/// A response that contains a success message.
#[derive(Debug, Serialize, Deserialize)]
pub struct SuccessResponse<T> {
    /// The success message.
    pub data: T,
}

impl<T> Deref for SuccessResponse<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

/// A response that contains an error message.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// The error message.
    pub message: String,
}

/// A response that can contain either a success or an error message.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ApiResponse<T> {
    /// A success response.
    Success(SuccessResponse<T>),

    /// An error response.
    Error(ErrorResponse),
}

impl<T> ApiResponse<T> {
    /// Creates a success response with the given data.
    #[must_use]
    pub const fn success(data: T) -> Self {
        ApiResponse::Success(SuccessResponse { data })
    }

    /// Creates an error response with the given message.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        ApiResponse::Error(ErrorResponse {
            message: message.into(),
        })
    }

    /// Returns true if the response is a success response.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, ApiResponse::Success(_))
    }
}

/// Creates an error response with the given message.
///
/// # Arguments
///
/// * `message` - The error message.
#[must_use]
pub fn api_error(message: impl Into<String>) -> ApiResponse<()> {
    ApiResponse::error(message)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_deserialize_success_response() {
        let json = r#"{"type":"Success","data":42}"#;
        let response: ApiResponse<i32> = serde_json::from_str(json).unwrap();
        match response {
            ApiResponse::Success(SuccessResponse { data }) => assert_eq!(data, 42),
            ApiResponse::Error(_) => panic!("Expected success response"),
        }
    }

    #[test]
    fn test_serialize_success_response() {
        let response = ApiResponse::Success(SuccessResponse { data: 42 });
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"type":"Success","data":42}"#);
    }

    #[test]
    fn test_deserialize_error_response() {
        let json = r#"{"type":"Error","message":"error message"}"#;
        let response: ApiResponse<i32> = serde_json::from_str(json).unwrap();
        match response {
            ApiResponse::Success(_) => panic!("Expected error response"),
            ApiResponse::Error(ErrorResponse { message }) => assert_eq!(message, "error message"),
        }
    }

    #[test]
    fn test_serialize_error_response() {
        let response = ApiResponse::<i32>::Error(ErrorResponse {
            message: "error message".to_string(),
        });
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"type":"Error","message":"error message"}"#);
    }
}
