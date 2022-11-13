use log::error;
use log::info;
use robotica_frontend::services::websocket::event_bus::Command;
use robotica_frontend::services::websocket::event_bus::EventBus;
use robotica_frontend::services::websocket::WsEvent;
use yew::functional::*;
use yew::prelude::*;
use yew_agent::Bridged;

#[function_component(Welcome)]
pub fn login() -> Html {
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
