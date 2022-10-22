use std::{cell::RefCell, rc::Rc};

use wasm_bindgen::prelude::*;
use yew::prelude::*;
use yew_router::prelude::*;

// use yew_router::{BrowserRouter, Routable, Switch};

mod components;
use components::chat::Chat;
use components::login::Login;

mod services;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Routable)]
pub enum Route {
    #[at("/")]
    Root,
    #[at("/chat")]
    Chat,
    #[at("/login")]
    Login,
    #[not_found]
    #[at("/404")]
    NotFound,
}

#[allow(clippy::let_unit_value)]
fn switch(selected_route: &Route) -> Html {
    match selected_route {
        Route::Root => html! {<Root/>},
        Route::Chat => html! {<Chat/>},
        Route::Login => html! {<Login/>},
        Route::NotFound => html! {<h1>{"404 Please ask a Penguin for help"}</h1>},
    }
}

pub type User = Rc<UserInner>;

#[derive(Debug, Eq, PartialEq)]
pub struct UserInner {
    pub name: RefCell<String>,
}

#[function_component(Root)]
fn root() -> Html {
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
                    { nav_bar() }
                    <div>
                        <Switch<Route> render={Switch::render(switch)}/>
                    </div>
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

fn nav_bar() -> Html {
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
                        <a class="nav-link active" aria-current="page" href="/">{ "Home" }</a>
                    </li>
                    <li class="nav-item">
                        <Link<Route> classes="nav-link" to={Route::Root}>{ "Home" }</Link<Route>>
                    </li>
                    <li class="nav-item">
                        <Link<Route> classes="nav-link" to={Route::Chat}>{ "Chat" }</Link<Route>>
                    </li>
                    <li class="nav-item">
                        <Link<Route> classes="nav-link" to={Route::Login}>{ "Login" }</Link<Route>>
                    </li>
                    <li class="nav-item dropdown">
                    <a class="nav-link dropdown-toggle" href="#" role="button" data-bs-toggle="dropdown" aria-expanded="false">
                        { "Dropdown" }
                    </a>
                    <ul class="dropdown-menu">
                        <li><a class="dropdown-item" href="#">{ "Action" }</a></li>
                        <li><a class="dropdown-item" href="#">{ "Another action" }</a></li>
                        <li><hr class="dropdown-divider"/></li>
                        <li><a class="dropdown-item" href="#">{ "Something else here" }</a></li>
                    </ul>
                    </li>
                    <li class="nav-item">
                    <a class="nav-link disabled">{ "Disabled" }</a>
                    </li>
                </ul>
                <form class="d-flex" role="search">
                    <input class="form-control me-2" type="search" placeholder="Search" aria-label="Search" />
                    <button class="btn btn-outline-success" type="submit">{ "Search" }</button>
                </form>
                </div>
            </div>
        </nav>
    }
}
