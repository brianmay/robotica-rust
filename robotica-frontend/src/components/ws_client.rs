//! Websocket client wrapper for Yew
use yew::prelude::*;

use crate::services::websocket::WebsocketService;

/// The yew properties for the websocket client
#[derive(Properties, PartialEq)]
pub struct Props {
    /// The children to render
    pub children: Children,
}

/// The websocket client wrapper
#[function_component(WsClient)]
pub fn ws_client(props: &Props) -> Html {
    let wss = use_state(WebsocketService::new);

    html! {
        <ContextProvider<WebsocketService> context={(*wss).clone()}>
            { for props.children.iter() }
        </ContextProvider<WebsocketService>>
    }
}
