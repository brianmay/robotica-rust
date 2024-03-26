//! Access locations table in the database
use geo::Geometry;
use geozero::wkb;
use robotica_common::robotica::locations::Location;
use tap::Pipe;
use tracing::error;

/// List locations from the database.
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
/// ```
/// use sqlx::PgPool;
/// use robotica_backend::database::locations::list_locations;
///
/// #[tokio::main]
/// async fn main() {
///     let postgres = PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
///     let locations = list_locations(&postgres).await.unwrap();
///     println!("{:?}", locations);
/// }
/// ```
pub async fn list_locations(postgres: &sqlx::PgPool) -> Result<Vec<Location>, sqlx::Error> {
    sqlx::query!(
        r#"SELECT id, name, color, announce_on_enter, announce_on_exit, bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>" FROM locations"#
    )
    .fetch_all(postgres)
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
    .pipe(Ok)
}

/// Create a new location in the database.
///
/// # Arguments
///
/// * `postgres` - The `PostgreSQL` connection pool.
/// * `location` - The location to create.
///
/// # Errors
///
/// This function can return an error if there is a problem with the database connection or query execution.
///
/// # Panics
///
/// This function may panic if the provided location is invalid.
///
/// # Examples
///
/// ```
/// use sqlx::PgPool;
/// use robotica_common::robotica::locations::CreateLocation;
/// use robotica_backend::database::locations::create_location;
///
/// #[tokio::main]
/// async fn main() {
///     let postgres = PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
///     let location = CreateLocation {
///         name: "New Location".to_string(),
///         color: "blue".to_string(),
///         announce_on_enter: true,
///         announce_on_exit: false,
///         bounds: geo::Polygon::new(geo::LineString::from(vec![(0.0,0.0), (0.0,1.0), (1.0,1.0), (0.0,0.0)]), vec![]),
///     };
///     let id = create_location(&postgres, &location).await.unwrap();
///     println!("Created location with ID: {}", id);
/// }
/// ```
pub async fn create_location(
    postgres: &sqlx::PgPool,
    location: &robotica_common::robotica::locations::CreateLocation,
) -> Result<i32, sqlx::Error> {
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
    .fetch_one(postgres)
    .await?
    .id
    .pipe(Ok)
}

/// Deletes a location from the database.
///
/// This function takes a `PostgreSQL` connection pool and an ID of a location, and deletes the location with the given ID from the database.
///
/// # Arguments
///
/// * `postgres` - A `PostgreSQL` connection pool.
/// * `id` - The ID of the location to delete.
///
/// # Returns
///
/// This function returns a `Result` which is an `Ok` if the location was successfully deleted, or an `Err` if an error occurred.
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
/// ```rust
/// use sqlx::PgPool;
/// use robotica_backend::database::locations::delete_location;
///
/// #[tokio::main]
/// async fn main() {
///     let postgres = PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
///     let id = 999; // ID of the location to delete
///     let rc = delete_location(&postgres, id).await;
///     if rc.is_ok() {
///         println!("Deleted location with ID: {}", id);
///     }
/// }
/// ```
pub async fn delete_location(postgres: &sqlx::PgPool, id: i32) -> Result<(), sqlx::Error> {
    sqlx::query!(r#"DELETE FROM locations WHERE id = $1"#, id)
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

/// Updates a location in the database.
///
/// This function takes a `PostgreSQL` connection pool and a location object, and updates the corresponding location in the database.
///
/// # Arguments
///
/// * `postgres` - A `PostgreSQL` connection pool.
/// * `location` - The location object containing the updated information.
///
/// # Returns
///
/// This function returns a `Result` which is an `Ok` if the location was successfully updated, or an `Err` if an error occurred.
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
/// ```rust
/// use sqlx::PgPool;
/// use robotica_backend::database::locations::update_location;
/// use robotica_common::robotica::locations::Location;
///
/// #[tokio::main]
/// async fn main() {
///     let postgres = PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
///     let location = Location {
///         id: 1,
///         name: "New Location".to_string(),
///         color: "blue".to_string(),
///         announce_on_enter: true,
///         announce_on_exit: false,
///         bounds: geo::Polygon::new(geo::LineString::from(vec![(0.0,0.0), (0.0,1.0), (1.0,1.0), (0.0,0.0)]), vec![]),
///     };
///     let rc = update_location(&postgres, &location).await;
///     if rc.is_ok() {
///         println!("Updated location with ID: {}", location.id);
///     }
/// }
/// ```
pub async fn update_location(
    postgres: &sqlx::PgPool,
    location: &robotica_common::robotica::locations::Location,
) -> Result<(), sqlx::Error> {
    let geometry = Geometry::Polygon(location.bounds.clone());
    let geo = wkb::Encode(geometry);

    sqlx::query!(
        r#"UPDATE locations SET name = $1, color = $2, announce_on_enter = $3, announce_on_exit = $4, bounds = $5 WHERE id = $6"#,
        location.name,
        location.color,
        location.announce_on_enter,
        location.announce_on_exit,
        geo as _,
        location.id
    )
    .execute(postgres)
    .await?
    .pipe(|rc| if rc.rows_affected() > 0 { Ok(()) } else { Err(sqlx::Error::RowNotFound) })
}

/// Retrieves a location from the database.
///
/// This function takes a `PostgreSQL` connection pool and an ID, and retrieves the corresponding location from the database.
///
/// # Arguments
///
/// * `postgres` - A `PostgreSQL` connection pool.
/// * `id` - The ID of the location to retrieve.
///
/// # Returns
///
/// This function returns a `Result` which is an `Ok` containing an `Option<Location>` if the location was found, or an `Err` if an error occurred.
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
/// ```rust
/// use sqlx::PgPool;
/// use robotica_backend::database::locations::get_location;
/// use robotica_common::robotica::locations::Location;
///
/// #[tokio::main]
/// async fn main() {
///     let postgres = PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
///     let id = 1;
///     let rc = get_location(&postgres, id).await;
///     println!("Location: {:?}", rc);
/// }
/// ```
pub async fn get_location(
    postgres: &sqlx::PgPool,
    id: i32,
) -> Result<Option<Location>, sqlx::Error> {
    let rc = sqlx::query!(
        r#"SELECT id, name, color, announce_on_enter, announce_on_exit, bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>" FROM locations WHERE id = $1"#,
        id
    )
    .fetch_optional(postgres)
    .await?;

    match rc {
        Some(row) => {
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

/// Searches for locations in the database based on a given location.
///
/// This function takes a `PostgreSQL` connection pool and a location, and searches for locations in the database that intersect with the given location.
///
/// # Arguments
///
/// * `postgres` - A `PostgreSQL` connection pool.
/// * `location` - The location to search for.
///
/// # Returns
///
/// This function returns a `Result` which is an `Ok` containing a vector of `Location` if the search was successful, or an `Err` if an error occurred.
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
/// ```rust
/// use sqlx::PgPool;
/// use robotica_backend::database::locations::search_locations;
/// use robotica_common::robotica::locations::Location;
/// use geo::Point;
///
/// #[tokio::main]
/// async fn main() {
///     let postgres = PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
///     let location = Point::new(1.0, 2.0);
///     let result = search_locations(&postgres, location).await;
///     println!("Locations: {:?}", result);
/// }
/// ```
pub async fn search_locations(
    postgres: &sqlx::PgPool,
    location: geo::Point<f64>,
    distance: f64,
) -> Result<Vec<Location>, sqlx::Error> {
    let geometry = Geometry::Point(location);
    let geo = wkb::Encode(geometry);

    sqlx::query!(
        r#"SELECT id, name, color, announce_on_enter, announce_on_exit, bounds as "bounds!: wkb::Decode<geo::Geometry<f64>>" FROM locations WHERE ST_DWithin($1, bounds, $2)"#,
        geo as _,
        distance,
    )
    .fetch_all(postgres)
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
    .pipe(Ok)
}
