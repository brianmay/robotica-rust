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

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::services::websocket::WebsocketService;
use gloo_net::http::Request;
use itertools::Itertools;
use robotica_common::config::Config;
use robotica_common::config::RoomConfig;
use tracing::info;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

use robotica_common::version;

use components::car::CarComponent;
use components::hot_water::HotWaterComponent;
use components::locations::locations_view::LocationsView;
use components::schedule_view::ScheduleView;
use components::tags_view::TagsView;
use components::welcome::Welcome;

use crate::components::rooms::Room;

#[derive(Debug, Clone, Eq, PartialEq, Routable)]
enum Route {
    #[at("/welcome")]
    Welcome,
    #[at("/room/:id")]
    Room { id: String },
    #[at("/car/:id")]
    Car { id: String },
    #[at("/hot_water/:id")]
    HotWater { id: String },
    #[at("/schedule")]
    Schedule,
    #[at("/tags")]
    Tags,
    #[at("/locations")]
    Locations,
    #[not_found]
    #[at("/404")]
    NotFound,
}

fn switch(selected_route: Route) -> Html {
    let content = match selected_route {
        Route::Welcome => html! {<Welcome/>},
        Route::Room { id } => html! { <Room id={id}/> },
        Route::Car { id } => html! { <CarComponent id={id}/> },
        Route::HotWater { id } => html! { <HotWaterComponent id={id}/> },
        Route::Schedule => html! { <ScheduleView/> },
        Route::Tags => html! { <TagsView/> },
        Route::Locations => return html! { <><NavBar/><LocationsView/></> },
        Route::NotFound => html! {<h1>{"404 Please ask a Penguin for help"}</h1>},
    };

    html! {
        <>
            <NavBar/>
            {content}
            {footer()}
        </>
    }
}

fn footer() -> Html {
    html! {
        <footer>
            <div>
                if let Some(build_date) = version::BUILD_DATE {
                    <div>{ format!("Built on {}", build_date)}</div>
                }
                if let Some(vcs_ref) = version::VCS_REF {
                    <div>{format!("VCS ref: {}", vcs_ref)}</div>
                }
            </div>
            <div>
                { "Robotica" }
            </div>
        </footer>
    }
}

enum AppMsg {
    Config(Arc<Config>),
}

struct App {
    wss: WebsocketService,
    config: Option<Arc<Config>>,
}

impl Component for App {
    type Message = AppMsg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let wss = WebsocketService::new();
        let link = ctx.link().clone();
        spawn_local(async move {
            let config = Arc::new(
                Request::get("/config")
                    .send()
                    .await
                    .unwrap()
                    .json::<Config>()
                    .await
                    .unwrap(),
            );
            link.send_message(AppMsg::Config(config));
        });
        App { wss, config: None }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            AppMsg::Config(config) => {
                self.config = Some(config);
                true
            }
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <ContextProvider<WebsocketService> context={self.wss.clone()}>
                <ContextProvider<Option<Arc<Config>>> context={&self.config}>
                    <BrowserRouter>
                        <Switch<Route> render={switch}/>
                    </BrowserRouter>
                </ContextProvider<Option<Arc<Config>>>>
            </ContextProvider<WebsocketService>>
        }
    }
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

#[function_component(NavBar)]
fn nav_bar() -> Html {
    let config = use_context::<Option<Arc<Config>>>().unwrap();

    let rooms = match &config {
        Some(config) => config
            .rooms
            .iter()
            .map(|room| (room.menu.as_str(), room))
            .into_group_map(),
        None => HashMap::new(),
    };

    let cars = match &config {
        Some(config) => config.cars.clone(),
        None => vec![],
    };

    let hot_water = match &config {
        Some(config) => config.hot_waters.clone(),
        None => vec![],
    };

    // turn menus into a vector of tuples sorted by menu name
    let menus: Vec<(&str, Vec<&RoomConfig>)> = rooms.into_iter().sorted_by_key(|x| x.0).collect();

    // get the current route
    let route = Rc::new(use_route::<Route>());

    let test_route = |link_route: &Route| match route.as_ref() {
        Some(route) => *route == *link_route,
        None => false,
    };

    let classes = |link_route: &Route| {
        let mut classes = classes!("nav-link");
        if test_route(link_route) {
            classes.push("active");
        }
        classes
    };

    let link = |link_route: Route, text| {
        html! {
            <Link<Route> classes={classes(&link_route)} to={link_route}>
                {text}
            </Link<Route>>
        }
    };

    let dropdown_classes = |link_route: &Route| {
        let mut classes = classes!("dropdown-item");
        if test_route(link_route) {
            classes.push("active");
        }
        classes
    };

    let dropdown_link = |link_route, text| {
        html! {
            <Link<Route> classes={dropdown_classes(&link_route)} to={link_route}>
                {text}
            </Link<Route>>
        }
    };

    html! {
        <nav class="navbar navbar-expand-sm navbar-dark bg-dark navbar-fixed-top">
            <div class="container-fluid">
                <a class="navbar-brand" href="/">{ "Robotica" }</a>
                <button class="navbar-toggler" type="button" data-bs-toggle="collapse" data-bs-target="#navbarSupportedContent" aria-controls="navbarSupportedContent" aria-expanded="false" aria-label="Toggle navigation">
                    <span class="navbar-toggler-icon"></span>
                </button>
                <div class="collapse navbar-collapse" id="navbarSupportedContent">
                    <ul class="navbar-nav me-auto mb-2 mb-lg-0">
                        <li class="nav-item">
                        { link(Route::Welcome, "Welcome") }
                        </li>
                        {
                            menus.iter().map(|(menu, rooms)| html! {
                                <li class="nav-item dropdown">
                                    <a class="nav-link dropdown-toggle" href="#" role="button" data-bs-toggle="dropdown" aria-expanded="false">
                                    {menu}
                                    </a>
                                    <ul class="dropdown-menu">
                                        { rooms.iter().map(|room| html! {
                                            <li>{dropdown_link(Route::Room {id: room.id.to_string()}, room.title.to_string())}</li>
                                        }).collect::<Html>() }
                                    </ul>
                                </li>
                            }).collect::<Html>()
                        }
                        <li class="nav-item dropdown">
                            <a class="nav-link dropdown-toggle" href="#" role="button" data-bs-toggle="dropdown" aria-expanded="false">
                            { "Cars" }
                            </a>
                            <ul class="dropdown-menu">
                                { cars.iter().map(|car| html! {
                                    <li>{dropdown_link(Route::Car {id: car.id.to_string()}, car.title.to_string())}</li>
                                }).collect::<Html>() }
                            </ul>
                        </li>
                        <li class="nav-item dropdown">
                            <a class="nav-link dropdown-toggle" href="#" role="button" data-bs-toggle="dropdown" aria-expanded="false">
                            { "Hot Waters" }
                            </a>
                            <ul class="dropdown-menu">
                                { hot_water.iter().map(|hot_water| html! {
                                    <li>{dropdown_link(Route::HotWater {id: hot_water.id.to_string()}, hot_water.title.to_string())}</li>
                                }).collect::<Html>() }
                            </ul>
                        </li>
                        <li class="nav-item">
                            { link(Route::Schedule, "Schedule") }
                        </li>
                        <li class="nav-item">
                            { link(Route::Tags, "Tags") }
                        </li>
                        <li class="nav-item">
                            { link(Route::Locations, "Locations") }
                        </li>
                        <li class="nav-item">
                            <a class="nav-link" href="/logout">{ "Logout" }</a>
                        </li>
                    </ul>
                </div>
            </div>
        </nav>
    }
}
