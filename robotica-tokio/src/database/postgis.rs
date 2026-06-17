//! `PostGIS` WKB encoding/decoding for sqlx 0.9.
//!
//! `geozero`'s built-in `with-postgis-sqlx` feature depends on `sqlx 0.8`, which conflicts
//! with this crate's `sqlx 0.9` dependency.  This module re-implements the same trait
//! impls against sqlx 0.9 using local newtype wrappers so there is no version conflict.
use geozero::wkb::{self, FromWkb, WkbDialect, WkbWriter};
use geozero::GeozeroGeometry;
use sqlx::encode::{Encode, IsNull};
use sqlx::error::BoxDynError;
use sqlx::postgres::{PgArgumentBuffer, PgHasArrayType, PgTypeInfo, PgValueRef, Postgres};
use sqlx::Decode;
use std::fmt;

/// Newtype wrapper for encoding a geometry value as `PostGIS` EWKB.
pub struct PgGeoEncode<T: GeozeroGeometry>(pub T);

/// Newtype wrapper for decoding a `PostGIS` EWKB geometry value.
pub struct PgGeoDecode<T: FromWkb> {
    /// The decoded geometry, or `None` when the column value is SQL `NULL`.
    pub geometry: Option<T>,
}

// -- Debug impls required by the sqlx query! macro --------------------------

impl<T: GeozeroGeometry> fmt::Debug for PgGeoEncode<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<geometry>")
    }
}

impl<T: FromWkb> fmt::Debug for PgGeoDecode<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<geometry>")
    }
}

// -- Encode -----------------------------------------------------------------

impl<T: GeozeroGeometry + Sized> sqlx::Type<Postgres> for PgGeoEncode<T> {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("geometry")
    }
}

impl<T: GeozeroGeometry + Sized> PgHasArrayType for PgGeoEncode<T> {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_geometry")
    }
}

impl<T: GeozeroGeometry + Sized> Encode<'_, Postgres> for PgGeoEncode<T> {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        let mut wkb_out: Vec<u8> = Vec::new();
        let mut writer = WkbWriter::with_opts(
            &mut wkb_out,
            WkbDialect::Ewkb,
            self.0.dims(),
            self.0.srid(),
            Vec::new(),
        );
        self.0
            .process_geom(&mut writer)
            .map_err(|e| sqlx::Error::Decode(e.to_string().into()))?;
        buf.extend(&wkb_out);
        Ok(IsNull::No)
    }
}

// -- Decode -----------------------------------------------------------------

impl<T: FromWkb + Sized> sqlx::Type<Postgres> for PgGeoDecode<T> {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("geometry")
    }
}

impl<T: FromWkb + Sized> PgHasArrayType for PgGeoDecode<T> {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_geometry")
    }
}

impl<'r, T: FromWkb + Sized> Decode<'r, Postgres> for PgGeoDecode<T> {
    fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        use sqlx::ValueRef as _;
        if value.is_null() {
            return Ok(Self { geometry: None });
        }
        let mut blob = <&[u8] as Decode<Postgres>>::decode(value)?;
        let geom = T::from_wkb(&mut blob, wkb::WkbDialect::Ewkb)
            .map_err(|e| sqlx::Error::Decode(e.to_string().into()))?;
        Ok(Self {
            geometry: Some(geom),
        })
    }
}
