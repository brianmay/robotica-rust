use axum::extract::Path;
use axum::routing::{delete, get, post, put};
use axum::{extract::State, Json};
use geo::{Geometry, Point};
use geozero::wkb;
use robotica_common::robotica::http_api::ApiResponse;
use robotica_common::robotica::locations::{CreateLocation, Location};
use tap::Pipe;
use tower_sessions::Session;
use tracing::error;

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

    sqlx::query!(
        r#"SELECT id, name, color, announce_on_enter, announce_on_exit, bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>" FROM locations"#
    )
    .fetch_all(&postgres)
    .await?
    .into_iter()
    .filter_map(|row| {
        if let Some(Geometry::Polygon(p)) = row.bounds.geometry {
            Location {
                id: row.id,
                name: row.name,
                bounds: p,
                color: row.color,
                announce_on_enter: row.announce_on_enter,
                announce_on_exit: row.announce_on_exit,
            }
            .pipe(Some)
        } else {
            error!("Not a polygon: {:?}", row.bounds);
            None
        }
    })
    .collect::<Vec<_>>()
    .pipe(ApiResponse::success)
    .pipe(Json)
    .pipe(Ok)
}

async fn create_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Json(location): Json<CreateLocation>,
) -> Result<Json<ApiResponse<Location>>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    let geometry = Geometry::Polygon(location.bounds.clone());
    let geo = wkb::Encode(geometry);

    sqlx::query!(
        r#"INSERT INTO locations (name, color, announce_on_enter, announce_on_exit, bounds) VALUES ($1, $2, $3, $4, $5) RETURNING id"#,
        location.name,
        location.color,
        location.announce_on_enter,
        location.announce_on_exit,
        geo as _
    )
    .fetch_one(&postgres)
    .await?
    .pipe(|id| Location {
        id: id.id,
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
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    let rc = sqlx::query!(r#"DELETE FROM locations WHERE id = $1"#, id)
        .execute(&postgres)
        .await?;

    if rc.rows_affected() == 0 {
        ResponseError::NotFoundError().pipe(Err)
    } else {
        ApiResponse::success(()).pipe(Json).pipe(Ok)
    }
}

async fn update_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Json(location): Json<Location>,
) -> Result<Json<ApiResponse<Location>>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    let geometry = Geometry::Polygon(location.bounds.clone());
    let geo = wkb::Encode(geometry);

    let rc = sqlx::query!(
        r#"UPDATE locations SET name = $1, color = $2, announce_on_enter = $3, announce_on_exit = $4, bounds = $5 WHERE id = $6"#,
        location.name,
        location.color,
        location.announce_on_enter,
        location.announce_on_exit,
        geo as _,
        location.id
    )
    .execute(&postgres)
    .await?;

    if rc.rows_affected() == 0 {
        ResponseError::NotFoundError().pipe(Err)
    } else {
        ApiResponse::success(location).pipe(Json).pipe(Ok)
    }
}

pub async fn get_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Path(id): Path<i32>,
) -> Result<Json<Location>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    let location = sqlx::query!(
        r#"SELECT id, name, color, announce_on_enter, announce_on_exit, bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>" FROM locations WHERE id = $1"#,
        id
    )
    .fetch_one(&postgres)
    .await.
    map_err(|err| if matches!(err, sqlx::Error::RowNotFound) {
        ResponseError::NotFoundError()
    } else {
        ResponseError::SqlError(err)
    })?;

    if let Some(Geometry::Polygon(p)) = location.bounds.geometry {
        Location {
            id: location.id,
            name: location.name,
            bounds: p,
            color: location.color,
            announce_on_enter: location.announce_on_enter,
            announce_on_exit: location.announce_on_exit,
        }
        .pipe(Json)
        .pipe(Ok)
    } else {
        error!("Not a polygon: {:?}", location.bounds);
        Err(ResponseError::internal_error("Not a polygon"))
    }
}

pub async fn search_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Json(location): Json<Point<f64>>,
) -> Result<Json<Vec<Location>>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    let geometry = Geometry::Point(location);
    let geo = wkb::Encode(geometry);

    sqlx::query!(
        r#"SELECT id, name, color, announce_on_enter, announce_on_exit, bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>" FROM locations WHERE ST_Intersects($1, bounds)"#,
        geo as _
    )
    .fetch_all(&postgres)
    .await?
    .into_iter()
    .filter_map(|row| {
        if let Some(Geometry::Polygon(p)) = row.bounds.geometry {
            Location {
                id: row.id,
                name: row.name,
                bounds: p,
                color: row.color,
                announce_on_enter: row.announce_on_enter,
                announce_on_exit: row.announce_on_exit,
            }
            .pipe(Some)
        } else {
            error!("Not a polygon: {:?}", row.bounds);
            None
        }
    })
    .collect::<Vec<_>>()
    .pipe(Json)
    .pipe(Ok)
}
