use yew::prelude::*;

use crate::services::websocket::WebsocketService;

#[derive(Properties, PartialEq)]
pub struct Props {
    pub children: Children,
}

#[function_component(WsClient)]
pub fn ws_client(props: &Props) -> Html {
    let wss = use_state(WebsocketService::new);

    html! {
        <ContextProvider<WebsocketService> context={(*wss).clone()}>
            { for props.children.iter() }
        </ContextProvider<WebsocketService>>
    }
}
