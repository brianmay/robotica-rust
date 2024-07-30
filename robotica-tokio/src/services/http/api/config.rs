use super::super::errors::ResponseError;
use super::super::{get_user, Config};
use axum::{extract::State, Json};
use robotica_common::config;
use std::sync::Arc;
use tower_sessions::Session;

#[allow(clippy::unused_async)]
pub async fn config_handler(
    State(rooms): State<Arc<config::Rooms>>,
    State(config): State<Arc<Config>>,
    session: Session,
) -> Result<Json<config::Config>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    let result = config::Config {
        rooms: rooms.as_ref().clone(),
        instance: config.instance.clone(),
    };

    Ok(Json(result))
}
