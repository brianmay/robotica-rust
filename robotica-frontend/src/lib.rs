//! Common yew frontend stuff for robotica
#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
// #![deny(clippy::unwrap_used)]
// #![deny(clippy::expect_used)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::use_self)]
// This code will not be used on concurrent threads.
#![allow(clippy::future_not_send)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::empty_docs)]

mod components;
mod robotica_wasm;
mod services;

use paste::paste;
use robotica_common::version;
use tracing::info;
use wasm_bindgen::prelude::*;
use yew_router::prelude::*;

use crate::components::app::App;

#[derive(Debug, Clone, Eq, PartialEq, Routable)]
enum Route {
    #[at("/")]
    Welcome,
    #[at("/room/:id")]
    Room { id: String },
    #[at("/car/:id")]
    Car { id: String },
    #[at("/water_heater/:id")]
    WaterHeater { id: String },
    #[at("/schedule")]
    Schedule,
    #[at("/tags")]
    Tags,
    #[at("/locations")]
    Locations,
    #[at("/occupancy")]
    Occupancy,
    #[not_found]
    #[at("/404")]
    NotFound,
}

/// The entry point for the frontend
#[wasm_bindgen(start)]
pub fn run() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    info!(
        "Starting robotica-frontend, version = {:?}, build time = {:?}",
        version::VCS_REF,
        version::BUILD_DATE
    );

    yew::Renderer::<App>::new().render();
    Ok(())
}
