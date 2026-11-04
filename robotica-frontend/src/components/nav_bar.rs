use itertools::Itertools;
use robotica_common::config::Config;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::services::websocket::{WebsocketService, WsEvent};
use crate::Route;

/// A view in the navigation bar.
#[derive(Clone)]
struct NavView {
    /// The view id, used as the route.
    id: String,

    /// The view title, displayed as the link text.
    title: String,
}

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

    // Split the views into dropdown menus (views with a menu) and top level
    // links (views without). The navbar is a pure function of the view list.
    let mut dropdown_views: HashMap<&str, Vec<NavView>> = HashMap::new();
    let mut top_level_views: Vec<NavView> = Vec::new();

    if let Some(config) = &config {
        for view in &config.views {
            let nav_view = NavView {
                id: view.id.clone(),
                title: view.title.clone(),
            };
            match &view.menu {
                Some(menu) => dropdown_views
                    .entry(menu.as_str())
                    .or_default()
                    .push(nav_view),
                None => top_level_views.push(nav_view),
            }
        }
    }

    // Sort menus by name; views within a menu and top level links stay in
    // declaration order.
    let mut dropdowns: Vec<(&str, Vec<NavView>)> = dropdown_views.into_iter().collect_vec();
    dropdowns.sort_by_key(|(menu, _)| *menu);

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
                            dropdowns.iter().map(|(menu, views)| html! {
                                <li class="nav-item dropdown">
                                    <a class="nav-link dropdown-toggle" href="#" role="button" data-bs-toggle="dropdown" aria-expanded="false">
                                    {*menu}
                                    </a>
                                    <ul class="dropdown-menu">
                                        { views.iter().map(|view| html! {
                                            <li onclick={close_menu.clone()}>{dropdown_link(Route::View {id: view.id.clone()}, view.title.clone())}</li>
                                        }).collect::<Html>() }
                                    </ul>
                                </li>
                            }).collect::<Html>()
                        }
                        {
                            top_level_views.iter().map(|view| html! {
                                <li class="nav-item" onclick={close_menu.clone()}>
                                    { nav_link(Route::View {id: view.id.clone()}, view.title.clone()) }
                                </li>
                            }).collect::<Html>()
                        }
                        <li class="nav-item">
                            <a class="nav-link" href="/logout">{ "Logout" }</a>
                        </li>
                    </ul>
                </div>
            </div>
        </nav>
    }
}
