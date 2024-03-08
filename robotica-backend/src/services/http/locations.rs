use axum::extract::Path;
use axum::routing::{delete, get, post, put};
use axum::{extract::State, Json};
use geo::{Geometry, Point};
use geozero::wkb;
use tap::Pipe;
use tower_sessions::Session;
use tracing::error;

use super::errors::ResponseError;
use super::{get_user, HttpState};

// #[derive(Debug)]
// pub struct DbLocation {
//     id: i32,
//     name: String,
//     bounds: wkb::Decode<geo::Geometry<f64>>,
// }

pub fn router(state: HttpState) -> axum::Router {
    axum::Router::new()
        .route("/list", get(list_handler))
        .route("/create", post(create_handler))
        .route("/update", put(update_handler))
        .route("/search", post(search_handler))
        .route("/:id", delete(delete_handler))
        .route("/:id", get(get_handler))
        .with_state(state)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Location {
    id: i32,
    name: String,
    bounds: geo::Polygon<f64>,
}

pub async fn list_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
) -> Result<Json<Vec<Location>>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    sqlx::query!(
        r#"SELECT id, name, bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>" FROM locations"#
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

pub async fn create_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Json(location): Json<Location>,
) -> Result<Json<i32>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    let geometry = Geometry::Polygon(location.bounds);
    let geo = wkb::Encode(geometry);

    sqlx::query!(
        r#"INSERT INTO locations (name, bounds) VALUES ($1, $2) RETURNING id"#,
        location.name,
        geo as _
    )
    .fetch_one(&postgres)
    .await?
    .id
    .pipe(Json)
    .pipe(Ok)
}

pub async fn delete_handler(
    Path(id): Path<i32>,
    State(postgres): State<sqlx::PgPool>,
    session: Session,
) -> Result<Json<()>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    sqlx::query!(r#"DELETE FROM locations WHERE id = $1"#, id)
        .execute(&postgres)
        .await?;

    ().pipe(Json).pipe(Ok)
}

pub async fn update_handler(
    State(postgres): State<sqlx::PgPool>,
    session: Session,
    Json(location): Json<Location>,
) -> Result<Json<()>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    let geometry = Geometry::Polygon(location.bounds);
    let geo = wkb::Encode(geometry);

    sqlx::query!(
        r#"UPDATE locations SET name = $1, bounds = $2 WHERE id = $3"#,
        location.name,
        geo as _,
        location.id
    )
    .execute(&postgres)
    .await?;

    ().pipe(Json).pipe(Ok)
}

pub async fn get_handler(
    Path(id): Path<i32>,
    State(postgres): State<sqlx::PgPool>,
    session: Session,
) -> Result<Json<Location>, ResponseError> {
    if get_user(&session).await.is_none() {
        return Err(ResponseError::AuthenticationFailed);
    };

    let location = sqlx::query!(
        r#"SELECT id, name, bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>" FROM locations WHERE id = $1"#,
        id
    )
    .fetch_one(&postgres)
    .await?;

    if let Some(Geometry::Polygon(p)) = location.bounds.geometry {
        Location {
            id: location.id,
            name: location.name,
            bounds: p,
        }
        .pipe(Json)
        .pipe(Ok)
    } else {
        error!("Not a polygon: {:?}", location.bounds);
        Err(ResponseError::InternalError("Not a polygon".to_string()))
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
        r#"SELECT id, name, bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>" FROM locations WHERE ST_Intersects($1, bounds)"#,
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
