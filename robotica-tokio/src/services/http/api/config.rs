use super::super::errors::ResponseError;
use super::super::{get_user, Config};
use axum::{extract::State, Json};
use robotica_common::config;
use robotica_common::robotica::entities::Id;
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
        // FIXME: This should not be hardcoded.
        cars: vec![config::CarConfig {
            id: Id::new("tesla_1"),
            title: "My Tesla".to_string(),
        }],
        // FIXME: This should not be hardcoded.
        hot_water: vec![config::HotWaterConfig {
            id: Id::new("hot_water"),
            title: "Hot Water Bathroom".to_string(),
        }],
    };

    Ok(Json(result))
}
