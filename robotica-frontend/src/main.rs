use std::{cell::RefCell, rc::Rc};

use yew::prelude::*;
use yew_router::{BrowserRouter, Routable, Switch};

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
        Route::NotFound => html! {<h1>{"404 baby"}</h1>},
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
        <ContextProvider<User> context={(*ctx).clone()}>
            <BrowserRouter>
                <div class="flex w-screen h-screen">
                    <Switch<Route> render={Switch::render(switch)}/>
                </div>
            </BrowserRouter>
        </ContextProvider<User>>
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    console_error_panic_hook::set_once();
    yew::start_app::<App>();
}
