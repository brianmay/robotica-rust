//! Access zones table in the database
use geo::Geometry;
use geozero::wkb;
use robotica_common::robotica::zones::Zone;
use tap::Pipe;
use tracing::error;

/// List zones from the database.
///
/// # Arguments
///
/// * `postgres` - The `PostgreSQL` connection pool.
///
/// # Errors
///
/// This function can return an error if there is a problem with the database connection or query execution.
///
/// # Examples
///
/// ```no_run
/// use sqlx::PgPool;
/// use robotica_tokio::database::zones::list_zones;
///
/// #[tokio::main]
/// async fn main() {
///     let postgres = PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
///     let zones = list_zones(&postgres).await.unwrap();
///     println!("{:?}", zones);
/// }
/// ```
pub async fn list_zones(postgres: &sqlx::PgPool) -> Result<Vec<Zone>, sqlx::Error> {
    sqlx::query!(
        r#"SELECT id, name, color, announce_on_enter, announce_on_exit, bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>" FROM zones"#
    )
    .fetch_all(postgres)
    .await?
    .into_iter()
    .filter_map(|row| {
        if let Some(Geometry::Polygon(p)) = row.bounds.geometry {
            Zone {
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
    .pipe(Ok)
}

/// Create a new zone in the database.
///
/// # Arguments
///
/// * `postgres` - The `PostgreSQL` connection pool.
/// * `zone` - The zone to create.
///
/// # Errors
///
/// This function can return an error if there is a problem with the database connection or query execution.
///
/// # Panics
///
/// This function may panic if the provided zone is invalid.
///
/// # Examples
///
/// ```no_run
/// use sqlx::PgPool;
/// use robotica_common::robotica::zones::CreateZone;
/// use robotica_tokio::database::zones::create_zone;
///
/// #[tokio::main]
/// async fn main() {
///     let postgres = PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
///     let zone = CreateZone {
///         name: "New Zone".to_string(),
///         color: "blue".to_string(),
///         announce_on_enter: true,
///         announce_on_exit: false,
///         bounds: geo::Polygon::new(geo::LineString::from(vec![(0.0,0.0), (0.0,1.0), (1.0,1.0), (0.0,0.0)]), vec![]),
///     };
///     let id = create_zone(&postgres, &zone).await.unwrap();
///     println!("Created zone with ID: {}", id);
/// }
/// ```
pub async fn create_zone(
    postgres: &sqlx::PgPool,
    zone: &robotica_common::robotica::zones::CreateZone,
) -> Result<i32, sqlx::Error> {
    let geometry = Geometry::Polygon(zone.bounds.clone());
    let geo = wkb::Encode(geometry);

    sqlx::query!(
        r#"INSERT INTO zones (name, color, announce_on_enter, announce_on_exit, bounds) VALUES ($1, $2, $3, $4, $5) RETURNING id"#,
        zone.name,
        zone.color,
        zone.announce_on_enter,
        zone.announce_on_exit,
        geo as _
    )
    .fetch_one(postgres)
    .await?
    .id
    .pipe(Ok)
}

/// Deletes a zone from the database.
///
/// This function takes a `PostgreSQL` connection pool and an ID of a zone, and deletes the zone with the given ID from the database.
///
/// # Arguments
///
/// * `postgres` - A `PostgreSQL` connection pool.
/// * `id` - The ID of the zone to delete.
///
/// # Returns
///
/// This function returns a `Result` which is an `Ok` if the zone was successfully deleted, or an `Err` if an error occurred.
///
/// # Errors
///
/// This function can return an error if there is a problem with the database connection or query execution.
///
/// # Panics
///
/// This function may panic if query! macro fails.
///
/// # Example
///
/// ```no_run
/// use sqlx::PgPool;
/// use robotica_tokio::database::zones::delete_zone;
///
/// #[tokio::main]
/// async fn main() {
///     let postgres = PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
///     let id = 999; // ID of the zone to delete
///     let rc = delete_zone(&postgres, id).await;
///     if rc.is_ok() {
///         println!("Deleted zone with ID: {}", id);
///     }
/// }
/// ```
pub async fn delete_zone(postgres: &sqlx::PgPool, id: i32) -> Result<(), sqlx::Error> {
    sqlx::query!(r#"DELETE FROM zones WHERE id = $1"#, id)
        .execute(postgres)
        .await?
        .pipe(|rc| {
            if rc.rows_affected() > 0 {
                Ok(())
            } else {
                Err(sqlx::Error::RowNotFound)
            }
        })
}

/// Updates a zone in the database.
///
/// This function takes a `PostgreSQL` connection pool and a zone object, and updates the corresponding zone in the database.
///
/// # Arguments
///
/// * `postgres` - A `PostgreSQL` connection pool.
/// * `zone` - The zone object containing the updated information.
///
/// # Returns
///
/// This function returns a `Result` which is an `Ok` if the zone was successfully updated, or an `Err` if an error occurred.
///
/// # Errors
///
/// This function can return an error if there is a problem with the database connection or query execution.
///
/// # Panics
///
/// This function may panic if the query! macro fails.
///
/// # Example
///
/// ```no_run
/// use sqlx::PgPool;
/// use robotica_tokio::database::zones::update_zone;
/// use robotica_common::robotica::zones::Zone;
///
/// #[tokio::main]
/// async fn main() {
///     let postgres = PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
///     let zone = Zone {
///         id: 1,
///         name: "New Zone".to_string(),
///         color: "blue".to_string(),
///         announce_on_enter: true,
///         announce_on_exit: false,
///         bounds: geo::Polygon::new(geo::LineString::from(vec![(0.0,0.0), (0.0,1.0), (1.0,1.0), (0.0,0.0)]), vec![]),
///     };
///     let rc = update_zone(&postgres, &zone).await;
///     if rc.is_ok() {
///         println!("Updated zone with ID: {}", zone.id);
///     }
/// }
/// ```
pub async fn update_zone(
    postgres: &sqlx::PgPool,
    zone: &robotica_common::robotica::zones::Zone,
) -> Result<(), sqlx::Error> {
    let geometry = Geometry::Polygon(zone.bounds.clone());
    let geo = wkb::Encode(geometry);

    sqlx::query!(
        r#"UPDATE zones SET name = $1, color = $2, announce_on_enter = $3, announce_on_exit = $4, bounds = $5 WHERE id = $6"#,
        zone.name,
        zone.color,
        zone.announce_on_enter,
        zone.announce_on_exit,
        geo as _,
        zone.id
    )
    .execute(postgres)
    .await?
    .pipe(|rc| if rc.rows_affected() > 0 { Ok(()) } else { Err(sqlx::Error::RowNotFound) })
}

/// Retrieves a zone from the database.
///
/// This function takes a `PostgreSQL` connection pool and an ID, and retrieves the corresponding zone from the database.
///
/// # Arguments
///
/// * `postgres` - A `PostgreSQL` connection pool.
/// * `id` - The ID of the zone to retrieve.
///
/// # Returns
///
/// This function returns a `Result` which is an `Ok` containing an `Option<Zone>` if the zone was found, or an `Err` if an error occurred.
///
/// # Errors
///
/// This function can return an error if there is a problem with the database connection or query execution.
///
/// # Panics
///
/// This function may panic if the query! macro fails.
///
/// # Example
///
/// ```no_run
/// use sqlx::PgPool;
/// use robotica_tokio::database::zones::get_zone;
/// use robotica_common::robotica::zones::Zone;
///
/// #[tokio::main]
/// async fn main() {
///     let postgres = PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
///     let id = 1;
///     let rc = get_zone(&postgres, id).await;
///     println!("Zone: {:?}", rc);
/// }
/// ```
pub async fn get_zone(
    postgres: &sqlx::PgPool,
    id: i32,
) -> Result<Option<Zone>, sqlx::Error> {
    let rc = sqlx::query!(
        r#"SELECT id, name, color, announce_on_enter, announce_on_exit, bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>" FROM zones WHERE id = $1"#,
        id
    )
    .fetch_optional(postgres)
    .await?;

    match rc {
        Some(row) => {
            if let Some(Geometry::Polygon(p)) = row.bounds.geometry {
                Zone {
                    id: row.id,
                    name: row.name,
                    bounds: p,
                    color: row.color,
                    announce_on_enter: row.announce_on_enter,
                    announce_on_exit: row.announce_on_exit,
                }
                .pipe(Some)
                .pipe(Ok)
            } else {
                // FIXME: Can we deal with this better?
                error!("Not a polygon: {:?}", row.bounds);
                Err(sqlx::Error::Protocol("Not a polygon".to_string()))
            }
        }
        None => Ok(None),
    }
}

/// Searches for zones in the database based on a given location.
///
/// This function takes a `PostgreSQL` connection pool and a location, and searches for zones in the database that intersect with the given location.
///
/// # Arguments
///
/// * `postgres` - A `PostgreSQL` connection pool.
/// * `location` - The location to search for.
///
/// # Returns
///
/// This function returns a `Result` which is an `Ok` containing a vector of `Zone` if the search was successful, or an `Err` if an error occurred.
///
/// # Errors
///
/// This function can return an error if there is a problem with the database connection or query execution.
///
/// # Panics
///
/// Clippy says this might panic.
///
/// # Example
///
/// ```no_run
/// use sqlx::PgPool;
/// use robotica_tokio::database::zones::search_zones;
/// use robotica_common::robotica::zones::Zone;
/// use geo::Point;
///
/// #[tokio::main]
/// async fn main() {
///     let postgres = PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
///     let location = Point::new(1.0, 2.0);
///     let result = search_zones(&postgres, location, 0.0).await;
///     println!("Zones: {:?}", result);
/// }
/// ```
pub async fn search_zones(
    postgres: &sqlx::PgPool,
    location: geo::Point<f64>,
    distance: f64,
) -> Result<Vec<Zone>, sqlx::Error> {
    let geometry = Geometry::Point(location);
    let geo = wkb::Encode(geometry);

    sqlx::query!(
        r#"SELECT id, name, color, announce_on_enter, announce_on_exit, bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>" FROM zones WHERE ST_DWithin($1, bounds, $2)"#,
        geo as _,
        distance,
    )
    .fetch_all(postgres)
    .await?
    .into_iter()
    .filter_map(|row| {
        if let Some(Geometry::Polygon(p)) = row.bounds.geometry {
            Zone {
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
    .pipe(Ok)
}

/// Search for zones within `candidate_radius` metres of `location`,
/// returning each with its signed distance to the zone boundary.
///
/// Distance convention:
/// - **negative** → tracker is *inside* the zone
/// - **positive** → tracker is *outside* the zone
///
/// # Errors
///
/// Returns a [`sqlx::Error`] on database failure.
pub async fn search_zones_with_distance(
    postgres: &sqlx::PgPool,
    location: geo::Point<f64>,
    candidate_radius: f64,
) -> Result<Vec<(Zone, f64)>, sqlx::Error> {
    let geometry = Geometry::Point(location);
    let geo = wkb::Encode(geometry);

    sqlx::query!(
        r#"SELECT id, name, color, announce_on_enter, announce_on_exit,
                  bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>",
                  ST_Distance($1::geography, bounds) AS "dist!: f64"
           FROM zones
           WHERE ST_DWithin($1::geography, bounds, $2)"#,
        geo as _,
        candidate_radius,
    )
    .fetch_all(postgres)
    .await?
    .into_iter()
    .filter_map(|row| {
        if let Some(Geometry::Polygon(p)) = row.bounds.geometry {
            let zone = Zone {
                id: row.id,
                name: row.name,
                bounds: p,
                color: row.color,
                announce_on_enter: row.announce_on_enter,
                announce_on_exit: row.announce_on_exit,
            };
            Some((zone, row.dist))
        } else {
            error!("Not a polygon: {:?}", row.bounds);
            None
        }
    })
    .collect::<Vec<_>>()
    .pipe(Ok)
}

mod test {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::wildcard_imports)]
    #![allow(unused_imports)]
    use sqlx::{Pool, Postgres};

    use super::*;
    use robotica_common::robotica::zones::CreateZone;

    #[allow(dead_code)]
    async fn db_create_zone(postgres: &Pool<Postgres>) {
        let bounds = geo::Polygon::new(
            geo::LineString::from(vec![(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (0.0, 0.0)]),
            vec![],
        );
        let geometry = Geometry::Polygon(bounds);
        let geo = wkb::Encode(geometry);

        sqlx::query("INSERT INTO zones (name,color,announce_on_enter,announce_on_exit, bounds) VALUES ('test', 'red', false, false, $1);",)
            .bind(geo)
            .execute(postgres)
            .await
            .unwrap();
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn test_create_zone(postgres: Pool<Postgres>) {
        let zone = CreateZone {
            name: "New Zone".to_string(),
            color: "blue".to_string(),
            announce_on_enter: true,
            announce_on_exit: false,
            bounds: geo::Polygon::new(
                geo::LineString::from(vec![(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (0.0, 0.0)]),
                vec![],
            ),
        };
        let id = create_zone(&postgres, &zone).await.unwrap();
        assert!(id > 0);
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn test_list_zones(postgres: Pool<Postgres>) {
        db_create_zone(&postgres).await;
        let zones = list_zones(&postgres).await.unwrap();
        assert!(!zones.is_empty());
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn test_get_zone(postgres: Pool<Postgres>) {
        db_create_zone(&postgres).await;
        let zones = list_zones(&postgres).await.unwrap();
        let zone = zones.first().unwrap();
        let rc = get_zone(&postgres, zone.id).await.unwrap();
        assert!(rc.is_some());
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn test_update_zone(postgres: Pool<Postgres>) {
        db_create_zone(&postgres).await;

        let zones = list_zones(&postgres).await.unwrap();
        let mut zone = zones.first().unwrap().clone();
        zone.name = "Updated Zone".to_string();
        update_zone(&postgres, &zone).await.unwrap();
        let rc = get_zone(&postgres, zone.id).await.unwrap().unwrap();
        assert_eq!(rc.name, "Updated Zone");
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn test_delete_zone(postgres: Pool<Postgres>) {
        db_create_zone(&postgres).await;

        let zones = list_zones(&postgres).await.unwrap();
        let zone = zones.first().unwrap();
        delete_zone(&postgres, zone.id).await.unwrap();
        let rc = get_zone(&postgres, zone.id).await.unwrap();
        assert!(rc.is_none());
    }
}
