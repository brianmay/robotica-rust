#![allow(clippy::let_unit_value)]

use robotica_frontend::services::websocket::WebsocketService;
use wasm_bindgen::prelude::*;
use yew::prelude::*;
use yew_router::prelude::*;

use robotica_common::version;

mod components;
use components::rooms::{
    AkiraRoom, Bathroom, BrianRoom, DiningRoom, JanRoom, LoungeRoom, Passage, TwinsRoom,
};
use components::schedule_view::ScheduleView;
use components::tags_view::TagsView;
use components::welcome::Welcome;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Routable)]
pub enum Route {
    #[at("/welcome")]
    Welcome,
    #[at("/brian")]
    BrianRoom,
    #[at("/jan")]
    JanRoom,
    #[at("/twins")]
    TwinsRoom,
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
    #[at("/schedule")]
    Schedule,
    #[at("/tags")]
    Tags,
    #[not_found]
    #[at("/404")]
    NotFound,
}

#[allow(clippy::let_unit_value)]
fn switch(selected_route: &Route) -> Html {
    let content = match selected_route {
        Route::Welcome => html! {<Welcome/>},
        Route::BrianRoom => html! { <BrianRoom/> },
        Route::JanRoom => html! { <JanRoom/> },
        Route::TwinsRoom => html! { <TwinsRoom/> },
        Route::AkiraRoom => html! { <AkiraRoom/> },
        Route::LoungeRoom => html! { <LoungeRoom/> },
        Route::DiningRoom => html! { <DiningRoom/> },
        Route::Bathroom => html! { <Bathroom/> },
        Route::Passage => html! { <Passage/> },
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

    html! {
        <ContextProvider<WebsocketService> context={wss}>
            <BrowserRouter>
                <Switch<Route> render={Switch::render(switch)}/>
            </BrowserRouter>
        </ContextProvider<WebsocketService>>
    }
}

#[wasm_bindgen(start)]
pub fn run() -> Result<(), JsValue> {
    wasm_logger::init(wasm_logger::Config::default());
    console_error_panic_hook::set_once();
    yew::start_app::<App>();
    Ok(())
}

#[function_component(NavBar)]
fn nav_bar() -> Html {
    let route: Option<Route> = match use_location() {
        Some(location) => location.route(),
        None => None,
    };

    let classes = |link_route| {
        let mut classes = classes!("nav-link");
        if Some(link_route) == route {
            classes.push("active");
        }
        classes
    };

    // let active = |link_route| Some(link_route) == route;

    let link = |link_route, text| {
        html! {
            <Link<Route> classes={classes(link_route)} to={link_route}>
                {text}
            </Link<Route>>
        }
    };

    let dropdown_classes = |link_route| {
        let mut classes = classes!("dropdown-item");
        if Some(link_route) == route {
            classes.push("active");
        }
        classes
    };

    let dropdown_link = |link_route, text| {
        html! {
            <Link<Route> classes={dropdown_classes(link_route)} to={link_route}>
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
                        <li class="nav-item dropdown">
                          <a class="nav-link dropdown-toggle" href="#" role="button" data-bs-toggle="dropdown" aria-expanded="false">
                            {"Bedrooms"}
                          </a>
                          <ul class="dropdown-menu">
                            <li>
                                { dropdown_link(Route::BrianRoom, "Brian's Room") }
                            </li>
                            <li>
                                { dropdown_link(Route::JanRoom, "Jan's Room") }
                            </li>
                            <li>
                                { dropdown_link(Route::TwinsRoom, "Twins' Room") }
                            </li>
                            <li>
                                { dropdown_link(Route::AkiraRoom, "Akira's Room") }
                            </li>
                          </ul>
                        </li>
                        <li class="nav-item dropdown">
                          <a class="nav-link dropdown-toggle" href="#" role="button" data-bs-toggle="dropdown" aria-expanded="false">
                            {"Common"}
                          </a>
                          <ul class="dropdown-menu">
                            <li>
                                { dropdown_link(Route::LoungeRoom, "Lounge Room") }
                            </li>
                            <li>
                                { dropdown_link(Route::DiningRoom, "Dining Room") }
                            </li>
                            <li>
                                { dropdown_link(Route::Bathroom, "Bathroom") }
                            </li>
                            <li>
                                { dropdown_link(Route::Passage, "Passage") }
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
