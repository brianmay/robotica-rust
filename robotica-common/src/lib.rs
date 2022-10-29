//! Common stuff shared between robotica-backend and robotica-frontend
#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::use_self)]

pub mod datetime;
pub mod mqtt;
pub mod scheduler;
pub mod user;
pub mod version;
pub mod websocket;
