use log::{error, info};
use yew::prelude::*;

use robotica_frontend::services::websocket::{
    event_bus::{Command, EventBus},
    WsEvent,
};
use yew_agent::Bridged;

/// The yew properties for the RequireConnection component
#[derive(Properties, PartialEq)]
pub struct Props {
    /// The children to render
    pub children: Children,
}

#[function_component(RequireConnection)]
pub fn require_connection(props: &Props) -> Html {
    let callback = {
        Callback::from(move |msg: ()| {
            error!("Received message: {:?}", msg);
        })
    };

    let events = use_mut_ref(|| EventBus::bridge(callback));
    let state = use_state(|| WsEvent::Disconnected("Not connected yet".to_string()));

    let callback = {
        let state = state.clone();

        Callback::from(move |msg: WsEvent| state.set(msg))
    };

    use_ref(|| {
        info!("Connecting to event handler");
        let msg = Command::EventHandler(callback);
        events.borrow_mut().send(msg);
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
