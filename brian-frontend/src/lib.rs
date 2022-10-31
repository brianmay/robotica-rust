#![allow(clippy::let_unit_value)]

use std::{cell::RefCell, rc::Rc};

mod components;
use components::rooms::{BrianRoom, DiningRoom};
use components::schedule_view::ScheduleView;
use components::tags_view::TagsView;
use components::welcome::Welcome;
use wasm_bindgen::prelude::*;
use yew::prelude::*;
use yew_router::prelude::*;

use robotica_common::version;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Routable)]
pub enum Route {
    #[at("/brian")]
    BrianRoom,
    #[at("/dining")]
    DiningRoom,
    #[at("/welcome")]
    Welcome,
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
        Route::BrianRoom => html! { <BrianRoom/> },
        Route::DiningRoom => html! { <DiningRoom/> },
        Route::Schedule => html! { <ScheduleView/> },
        Route::Tags => html! { <TagsView/> },
        Route::Welcome => html! {<Welcome/>},
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

pub type User = Rc<UserInner>;

#[derive(Debug, Eq, PartialEq)]
pub struct UserInner {
    pub name: RefCell<String>,
}

#[function_component(Test)]
fn test() -> Html {
    let user = use_context::<User>().expect("No context found.");
    let name = user.name.borrow();

    html! {
        <div>
        <h1>{ format!("Hello {}", name) }</h1>
        <p>{ "It is me again!"}</p>
        <p>{ "I am a paragraph."}</p>
        </div>
    }
}

#[function_component(App)]
fn app() -> Html {
    let ctx = use_state(|| {
        Rc::new(UserInner {
            name: RefCell::new("Anonymous".into()),
        })
    });

    html! {
        <>
            <ContextProvider<User> context={(*ctx).clone()}>
                <BrowserRouter>
                   <Switch<Route> render={Switch::render(switch)}/>
                </BrowserRouter>
            </ContextProvider<User>>
        </>
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
                    <li class="nav-item">
                        { link(Route::BrianRoom, "Brian's Room") }
                    </li>
                    <li class="nav-item">
                        { link(Route::DiningRoom, "Dining Room") }
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
