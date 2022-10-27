use log::{error, info};
use yew::prelude::*;

use robotica_frontend::services::websocket::{Command, WebsocketService, WsEvent};

/// The yew properties for the RequireConnection component
#[derive(Properties, PartialEq)]
pub struct Props {
    /// The children to render
    pub children: Children,
}

#[function_component(RequireConnection)]
pub fn require_connection(props: &Props) -> Html {
    let state = use_state(|| WsEvent::Disconnected("Not connected yet".to_string()));

    let callback = {
        let state = state.clone();

        Callback::from(move |msg: WsEvent| state.set(msg))
    };

    let wss = use_context::<WebsocketService>().expect("No context found.");
    use_ref(|| {
        info!("Connecting to event handler");
        let msg = Command::EventHandler(callback);
        let mut tx = wss.tx;
        tx.try_send(msg)
            .unwrap_or_else(|_| error!("Failed to register event handler"));
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
        WsEvent::FatalError(reason) => {
            html!(
                <div class="alert alert-danger">
                    {"Fatal Error: "} {reason}
                </div>
            )
        }
    }
}
