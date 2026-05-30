use crate::services::websocket::WebsocketService;
use crate::services::websocket::WsEvent;
use wasm_bindgen_futures::spawn_local;
use web_sys::window;
use yew::functional::{function_component, use_context, use_mut_ref, use_state};
use yew::prelude::*;

#[function_component(Welcome)]
pub fn login() -> Html {
    let wss: WebsocketService = use_context().unwrap();
    let subscription = use_mut_ref(|| None);

    let state = use_state(|| WsEvent::Disconnected("Not connected yet".to_string()));

    let callback = {
        let state = state.clone();
        Callback::from(move |msg: WsEvent| state.set(msg))
    };

    use_mut_ref(move || {
        let mut wss = wss;
        spawn_local(async move {
            let sub = wss.subscribe_events(callback).await;
            *subscription.borrow_mut() = Some(sub);
        });
    });

    match &*state {
        WsEvent::Connected { user, version } => {
            html!(
                <>
                    <h1>{ "Welcome " } { &user.name }</h1>
                    <div>{"sub: "} { &user.oidc_id } </div>
                    <div>{"email: "} { &user.email } </div>
                    <div>{if user.is_admin { "Admin" } else { "Not admin" }}</div>
                    <div class="backend">{format!("Connected to backend version {}", version)}</div>
                </>
            )
        }
        WsEvent::LoginRequired { login_url } => {
            let login_url = login_url.clone();
            let onclick = Callback::from(move |_| {
                let login_url = login_url.clone();
                spawn_local(async move {
                    if let Some(window) = window() {
                        let _ = window.location().set_href(&login_url);
                    }
                });
            });
            html!(
                <div>
                    <h1>{ "Robotica" }</h1>
                    <p>{ "You need to log in to continue." }</p>
                    <button class="btn btn-primary" {onclick}>{ "Login" }</button>
                </div>
            )
        }
        WsEvent::Disconnected(reason) => {
            html!(
                <h1>{ "Disconnected: " } {reason}</h1>
            )
        }
    }
}
