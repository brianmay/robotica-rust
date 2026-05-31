use itertools::Itertools;
use robotica_common::config::Config;
use robotica_common::config::RoomConfig;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::services::websocket::{WebsocketService, WsEvent};
use crate::Route;

#[function_component(NavBar)]
pub fn nav_bar() -> Html {
    let wss: WebsocketService = use_context().unwrap();
    let config = use_context::<Option<Arc<Config>>>().unwrap();
    let menu_open = use_state(|| false);
    let show_full_ui = use_state(|| false);
    let subscription = use_mut_ref(|| None);

    let toggle_menu = {
        let menu_open = menu_open.clone();
        Callback::from(move |_| menu_open.set(!*menu_open))
    };
    let close_menu = {
        let menu_open = menu_open.clone();
        Callback::from(move |_| menu_open.set(false))
    };

    {
        let show_full_ui = show_full_ui.clone();
        let callback = Callback::from(move |msg: WsEvent| {
            show_full_ui.set(matches!(
                msg,
                WsEvent::Connected { .. } | WsEvent::Disconnected(..)
            ));
        });

        let mut wss = wss;
        use_mut_ref(move || {
            spawn_local(async move {
                let sub = wss.subscribe_events(callback).await;
                *subscription.borrow_mut() = Some(sub);
            });
        });
    }

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

    let water_heaters = match &config {
        Some(config) => config.water_heaters.clone(),
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

    let nav_link = |link_route: Route, text| {
        html! {
            <Link<Route> classes={classes(&link_route)} to={link_route}>
                {text}
            </Link<Route>>
        }
    };

    if !*show_full_ui {
        return html! {
            <nav class="navbar navbar-expand-sm navbar-dark bg-dark navbar-fixed-top">
                <div class="container-fluid">
                    <Link<Route> classes={ classes!("navbar-brand") } to={Route::Welcome}>
                        { "Robotica" }
                    </Link<Route>>
                </div>
            </nav>
        };
    }

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
                <Link<Route> classes={ classes!("navbar-brand") } to={Route::Welcome}>
                    { "Robotica" }
                </Link<Route>>
                <button class="navbar-toggler" type="button" onclick={toggle_menu} aria-controls="navbarSupportedContent" aria-expanded={(*menu_open).to_string()} aria-label="Toggle navigation">
                    <span class="navbar-toggler-icon"></span>
                </button>
                <div class={classes!("collapse", "navbar-collapse", if *menu_open { "show" } else { "" })}>
                    <ul class="navbar-nav me-auto mb-2 mb-lg-0">
                        {
                            menus.iter().map(|(menu, rooms)| html! {
                                <li class="nav-item dropdown">
                                    <a class="nav-link dropdown-toggle" href="#" role="button" data-bs-toggle="dropdown" aria-expanded="false">
                                    {*menu}
                                    </a>
                                    <ul class="dropdown-menu">
                                        { rooms.iter().map(|room| html! {
                                            <li onclick={close_menu.clone()}>{dropdown_link(Route::Room {id: room.id.clone()}, room.title.clone())}</li>
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
                                    <li onclick={close_menu.clone()}>{dropdown_link(Route::Car {id: car.id.to_string()}, car.title.clone())}</li>
                                }).collect::<Html>() }
                            </ul>
                        </li>
                        <li class="nav-item dropdown">
                            <a class="nav-link dropdown-toggle" href="#" role="button" data-bs-toggle="dropdown" aria-expanded="false">
                            { "Water Heaters" }
                            </a>
                            <ul class="dropdown-menu">
                                { water_heaters.iter().map(|water_heater| html! {
                                    <li onclick={close_menu.clone()}>{dropdown_link(Route::WaterHeater {id: water_heater.id.to_string()}, water_heater.title.clone())}</li>
                                }).collect::<Html>() }
                            </ul>
                        </li>
                        <li class="nav-item" onclick={close_menu.clone()}>
                            { nav_link(Route::Schedule, "Schedule") }
                        </li>
                        <li class="nav-item" onclick={close_menu.clone()}>
                            { nav_link(Route::Tags, "Tags") }
                        </li>
                        <li class="nav-item" onclick={close_menu.clone()}>
                            { nav_link(Route::Locations, "Locations") }
                        </li>
                        <li class="nav-item" onclick={close_menu.clone()}>
                            { nav_link(Route::Occupancy, "Occupancy") }
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
