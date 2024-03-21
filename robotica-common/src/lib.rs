//! Common stuff shared between robotica-backend and robotica-frontend
#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::use_self)]

pub mod anavi_thermometer;
pub mod config;
pub mod controllers;
pub mod mqtt;
pub mod robotica;
pub mod shelly;
pub mod teslamate;
pub mod user;
pub mod version;
pub mod websocket;
pub mod zigbee2mqtt;
pub mod zwave;

#[cfg(feature = "websockets")]
pub mod protobuf;

#[cfg(feature = "websockets")]
mod protos;

#[cfg(feature = "chrono")]
pub mod datetime;

#[cfg(feature = "chrono")]
pub mod scheduler;

pub use chrono::NaiveTime;
pub use chrono::TimeDelta;
pub use std::time::Duration;
