use robotica_frontend::services::websocket::{WebsocketService, WsEvent};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

/// The yew properties for the RequireConnection component
#[derive(Properties, PartialEq)]
pub struct Props {
    /// The children to render
    pub children: Children,
}

#[function_component(RequireConnection)]
pub fn require_connection(props: &Props) -> Html {
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
                    { for props.children.iter() }
                    <div class="backend">{format!("Connected to backend as {} with version {}", user, version)}</div>
                </>
            )
        }
        WsEvent::Disconnected(reason) => {
            html!(
                <div class="alert alert-warning">
                    {"Disconnected: "} {reason}
                </div>
            )
        }
    }
}
