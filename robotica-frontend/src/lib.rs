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

mod components;
mod services;

use std::collections::HashMap;
use std::sync::Arc;

use crate::services::websocket::WebsocketService;
use gloo_net::http::Request;
use itertools::Itertools;
use robotica_common::config::RoomConfig;
use robotica_common::config::Rooms;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

use robotica_common::version;

use components::rooms::{
    AkiraRoom, Bathroom, BrianRoom, ColinRoom, DiningRoom, JanRoom, LoungeRoom, Passage, Tesla,
    TwinsRoom,
};
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
    #[at("/brian")]
    BrianRoom,
    #[at("/jan")]
    JanRoom,
    #[at("/twins")]
    TwinsRoom,
    #[at("/colin")]
    ColinRoom,
    #[at("/akira")]
    AkiraRoom,
    #[at("/lounge")]
    LoungeRoom,
    #[at("/dining")]
    DiningRoom,
    #[at("/bathroom")]
    Bathroom,
    #[at("/passage")]
    Passage,
    #[at("/tesla")]
    Tesla,
    #[at("/schedule")]
    Schedule,
    #[at("/tags")]
    Tags,
    #[not_found]
    #[at("/404")]
    NotFound,
}

fn switch(selected_route: Route) -> Html {
    let content = match selected_route {
        Route::Welcome => html! {<Welcome/>},
        Route::Room { id } => html! { <Room id={id}/> },
        Route::BrianRoom => html! { <BrianRoom/> },
        Route::JanRoom => html! { <JanRoom/> },
        Route::TwinsRoom => html! { <TwinsRoom/> },
        Route::ColinRoom => html! { <ColinRoom/> },
        Route::AkiraRoom => html! { <AkiraRoom/> },
        Route::LoungeRoom => html! { <LoungeRoom/> },
        Route::DiningRoom => html! { <DiningRoom/> },
        Route::Bathroom => html! { <Bathroom/> },
        Route::Passage => html! { <Passage/> },
        Route::Tesla => html! { <Tesla/> },
        Route::Schedule => html! { <ScheduleView/> },
        Route::Tags => html! { <TagsView/> },
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

#[function_component(App)]
fn app() -> Html {
    let wss = WebsocketService::new();

    let rooms = use_state(|| None);

    let rooms_setter = rooms.clone();
    spawn_local(async move {
        let rooms = Arc::new(
            Request::get("/rooms")
                .send()
                .await
                .unwrap()
                .json::<Rooms>()
                .await
                .unwrap(),
        );
        rooms_setter.set(Some(rooms));
    });

    html! {
        <ContextProvider<WebsocketService> context={wss}>
            <ContextProvider<Option<Arc<Rooms>>> context={&*rooms}>
                <BrowserRouter>
                    <Switch<Route> render={switch}/>
                </BrowserRouter>
            </ContextProvider<Option<Arc<Rooms>>>>
        </ContextProvider<WebsocketService>>
    }
}

/// The entry point for the frontend
#[wasm_bindgen(start)]
pub fn run() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();
    yew::Renderer::<App>::new().render();
    Ok(())
}

#[function_component(NavBar)]
fn nav_bar() -> Html {
    let rooms = use_context::<Option<Arc<Rooms>>>().unwrap();

    let rooms = match rooms {
        Some(rooms) => rooms,
        None => Arc::new(Vec::new()),
    };

    let menus: HashMap<&str, Vec<&RoomConfig>> = rooms
        .iter()
        .map(|room| (room.menu.as_str(), room))
        .into_group_map();

    // turn menus into a vector of tuples sorted by menu name
    let menus: Vec<(&str, Vec<&RoomConfig>)> = menus.into_iter().sorted_by_key(|x| x.0).collect();

    let route: Option<&Route> = match use_location() {
        Some(location) => location.state().map(|state| *state),
        None => None,
    };

    let classes = |link_route: &Route| {
        let mut classes = classes!("nav-link");
        if Some(link_route) == route {
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
        if Some(link_route) == route {
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
                            {"Bedrooms"}
                          </a>
                          <ul class="dropdown-menu">
                            <li>
                                { dropdown_link(Route::BrianRoom, "Brian's Room".to_string()) }
                            </li>
                            <li>
                                { dropdown_link(Route::JanRoom, "Jan's Room".to_string()) }
                            </li>
                            <li>
                                { dropdown_link(Route::TwinsRoom, "Twins' Room".to_string()) }
                            </li>
                            <li>
                                { dropdown_link(Route::ColinRoom, "Colin's Room".to_string()) }
                            </li>
                            <li>
                                { dropdown_link(Route::AkiraRoom, "Akira's Room".to_string()) }
                            </li>
                          </ul>
                        </li>
                        <li class="nav-item dropdown">
                          <a class="nav-link dropdown-toggle" href="#" role="button" data-bs-toggle="dropdown" aria-expanded="false">
                            {"Common"}
                          </a>
                          <ul class="dropdown-menu">
                            <li>
                                { dropdown_link(Route::LoungeRoom, "Lounge Room".to_string()) }
                            </li>
                            <li>
                                { dropdown_link(Route::DiningRoom, "Dining Room".to_string()) }
                            </li>
                            <li>
                                { dropdown_link(Route::Bathroom, "Bathroom".to_string()) }
                            </li>
                            <li>
                                { dropdown_link(Route::Passage, "Passage".to_string()) }
                            </li>
                            <li>
                                { dropdown_link(Route::Tesla, "Tesla".to_string()) }
                            </li>
                          </ul>
                        </li>
                        <li class="nav-item">
                            { link(Route::Schedule, "Schedule") }
                        </li>
                        <li class="nav-item">
                            { link(Route::Tags, "Tags") }
                        </li>
                    </ul>
                </div>
            </div>
        </nav>
    }
}
