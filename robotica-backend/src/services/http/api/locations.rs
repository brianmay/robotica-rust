use axum::extract::Path;
use axum::routing::{delete, get, post, put};
use axum::{extract::State, Json};
use geo::Point;
use robotica_common::robotica::http_api::ApiResponse;
use robotica_common::robotica::locations::{CreateLocation, Location};
use tap::Pipe;
use tower_sessions::Session;

use crate::database::locations::{
    create_location, delete_location, get_location, list_locations, search_locations,
    update_location,
};

use super::super::{get_user, HttpState};
use super::errors::ResponseError;

pub fn router(state: HttpState) -> axum::Router {
    axum::Router::new()
        .route("/", get(list_handler))
        .route("/", put(update_handler))
        .route("/create", post(create_handler))
        .route("/search", post(search_handler))
        .route("/:id", delete(delete_handler))
        .route("/:id", get(get_handler))
        .with_state(state)
}

pub async fn list_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
) -> Result<Json<ApiResponse<Vec<Location>>>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    list_locations(&postgres)
        .await?
        .pipe(ApiResponse::success)
        .pipe(Json)
        .pipe(Ok)
}

async fn create_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Json(location): Json<CreateLocation>,
) -> Result<Json<ApiResponse<Location>>, ResponseError> {
    let Some(user) = get_user(&session).await else {
        return Err(ResponseError::AuthenticationFailed);
    };

    if !user.is_admin {
        return Err(ResponseError::AuthorizationFailed);
    }

    create_location(&postgres, &location)
        .await?
        .pipe(|id| Location {
            id,
            name: location.name,
            bounds: location.bounds,
            color: location.color,
            announce_on_enter: location.announce_on_enter,
            announce_on_exit: location.announce_on_exit,
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

    delete_location(&postgres, id)
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
    Json(location): Json<Location>,
) -> Result<Json<ApiResponse<Location>>, ResponseError> {
    let Some(user) = get_user(&session).await else {
        return Err(ResponseError::AuthenticationFailed);
    };

    if !user.is_admin {
        return Err(ResponseError::AuthorizationFailed);
    }

    update_location(&postgres, &location)
        .await
        .map_err(|err| {
            if matches!(err, sqlx::Error::RowNotFound) {
                ResponseError::NotFoundError()
            } else {
                ResponseError::SqlError(err)
            }
        })?
        .pipe(|()| location)
        .pipe(ApiResponse::success)
        .pipe(Json)
        .pipe(Ok)
}

pub async fn get_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Path(id): Path<i32>,
) -> Result<Json<Location>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    get_location(&postgres, id)
        .await?
        .map_or(Err(ResponseError::NotFoundError()), |location| {
            Ok(Json(location))
        })
}

pub async fn search_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Json(location): Json<Point<f64>>,
) -> Result<Json<Vec<Location>>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    search_locations(&postgres, location)
        .await?
        .pipe(Json)
        .pipe(Ok)
}
