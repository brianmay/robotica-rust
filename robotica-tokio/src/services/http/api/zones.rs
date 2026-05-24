use axum::extract::Path;
use axum::routing::{delete, get, post, put};
use axum::{extract::State, Json};
use geo::Point;
use robotica_common::robotica::http_api::ApiResponse;
use robotica_common::robotica::zones::{CreateZone, Zone};
use tap::Pipe;
use tower_sessions::Session;

use crate::database::zones::{
    create_zone, delete_zone, get_zone, list_zones, search_zones, update_zone,
};

use super::super::{get_user, HttpState};
use super::errors::ResponseError;

pub fn router(state: HttpState) -> axum::Router {
    axum::Router::new()
        .route("/", get(list_handler))
        .route("/", put(update_handler))
        .route("/create", post(create_handler))
        .route("/search", post(search_handler))
        .route("/{id}", delete(delete_handler))
        .route("/{id}", get(get_handler))
        .with_state(state)
}

pub async fn list_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
) -> Result<Json<ApiResponse<Vec<Zone>>>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    }

    list_zones(&postgres)
        .await?
        .pipe(ApiResponse::success)
        .pipe(Json)
        .pipe(Ok)
}

async fn create_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Json(zone): Json<CreateZone>,
) -> Result<Json<ApiResponse<Zone>>, ResponseError> {
    let Some(user) = get_user(&session).await else {
        return Err(ResponseError::AuthenticationFailed);
    };

    if !user.is_admin {
        return Err(ResponseError::AuthorizationFailed);
    }

    create_zone(&postgres, &zone)
        .await?
        .pipe(|id| Zone {
            id,
            name: zone.name,
            bounds: zone.bounds,
            color: zone.color,
            announce_on_enter: zone.announce_on_enter,
            announce_on_exit: zone.announce_on_exit,
        })
        .pipe(ApiResponse::success)
        .pipe(Json)
        .pipe(Ok)
}

async fn delete_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<()>>, ResponseError> {
    let Some(user) = get_user(&session).await else {
        return Err(ResponseError::AuthenticationFailed);
    };

    if !user.is_admin {
        return Err(ResponseError::AuthorizationFailed);
    }

    delete_zone(&postgres, id)
        .await
        .map_err(|err| {
            if matches!(err, sqlx::Error::RowNotFound) {
                ResponseError::NotFoundError()
            } else {
                ResponseError::SqlError(err)
            }
        })?
        .pipe(ApiResponse::success)
        .pipe(Json)
        .pipe(Ok)
}

async fn update_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Json(zone): Json<Zone>,
) -> Result<Json<ApiResponse<Zone>>, ResponseError> {
    let Some(user) = get_user(&session).await else {
        return Err(ResponseError::AuthenticationFailed);
    };

    if !user.is_admin {
        return Err(ResponseError::AuthorizationFailed);
    }

    update_zone(&postgres, &zone)
        .await
        .map_err(|err| {
            if matches!(err, sqlx::Error::RowNotFound) {
                ResponseError::NotFoundError()
            } else {
                ResponseError::SqlError(err)
            }
        })?
        .pipe(|()| zone)
        .pipe(ApiResponse::success)
        .pipe(Json)
        .pipe(Ok)
}

pub async fn get_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Path(id): Path<i32>,
) -> Result<Json<Zone>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    }

    get_zone(&postgres, id)
        .await?
        .map_or(Err(ResponseError::NotFoundError()), |zone| Ok(Json(zone)))
}

pub async fn search_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Json(location): Json<Point<f64>>,
) -> Result<Json<Vec<Zone>>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    }

    search_zones(&postgres, location, 0.0)
        .await?
        .pipe(Json)
        .pipe(Ok)
}
