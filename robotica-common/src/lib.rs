//! Common stuff shared between robotica-backend and robotica-frontend
#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::use_self)]

pub mod anavi_thermometer;
pub mod controllers;
pub mod mqtt;
pub mod user;
pub mod version;
pub mod websocket;
pub mod zigbee2mqtt;

#[cfg(feature = "chrono")]
pub mod datetime;

#[cfg(feature = "chrono")]
pub mod scheduler;
