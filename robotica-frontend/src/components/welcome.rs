use crate::services::websocket::WebsocketService;
use crate::services::websocket::WsEvent;
use wasm_bindgen_futures::spawn_local;
use yew::functional::*;
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
                    <div>{"sub: "} { &user.sub } </div>
                    <div>{"email: "} { &user.email } </div>
                    <div class="backend">{format!("Connected to backend version {}", version)}</div>
                </>
            )
        }
        WsEvent::Disconnected(reason) => {
            html!(
                <h1>{ "Disconnected: " } {reason}</h1>
            )
        }
    }
}
