//! Helpers for calling the robotica HTTP API.

use gloo_net::http::{Request, Response};
use gloo_net::Error;
use robotica_common::robotica::http_api::ApiResponse;
use serde::de::DeserializeOwned;
use tap::Pipe;
use tracing::error;

/// Get the UI configuration for the logged in user.
///
/// This is the default profile filtered to the views the user's groups
/// allow; the navigation bar renders exactly these views.
///
/// # Errors
///
/// Returns an error message string if the request fails, the user is not
/// authenticated, or the server reports an error.
pub async fn get_ui_config() -> Result<robotica_common::config::Config, String> {
    process_response(Request::get("/api/config").send().await).await
}

/// Process an API response, unwrapping the [`ApiResponse`] envelope.
///
/// # Errors
///
/// Returns an error message string if the request failed, the response
/// was an error, or the body could not be parsed.
pub async fn process_response<T: DeserializeOwned>(
    response: Result<Response, Error>,
) -> Result<T, String> {
    match response {
        Ok(response) => {
            let api_response: Option<ApiResponse<T>> = response
                .json()
                .await
                .map_err(|err| error!("Error parsing server response: {err:?}"))
                .ok();

            match (response.ok(), api_response) {
                (true, Some(ApiResponse::Success(response))) => Ok(response.data),
                (true, None) => Err("Invalid response".to_string()),
                (_, Some(ApiResponse::Error(err))) => err.message.pipe(Err),
                (false, _) => response.status_text().pipe(Err),
            }
        }
        Err(err) => format!("http error: {err:?}").pipe(Err),
    }
}
