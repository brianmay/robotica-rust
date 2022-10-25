use log::{error, info};
use yew::prelude::*;

use crate::services::controllers::{Action, Icon};
use crate::services::websocket::{Command, WebsocketService, WsEvent};

use super::button::{Button, LightProps, MusicProps, SwitchProps};

#[function_component(Lights)]
pub fn lights_view() -> Html {
    let light_icon = Icon::new("light");
    let wake_up_icon = Icon::new("wake_up");
    let fan_icon = Icon::new("fan");
    let tv_icon = Icon::new("tv");

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
        WsEvent::Connected => {
            html!(
                <>
                    <div class="flex flex-row">
                        <h2>{"Brian's Room"}</h2>
                        <Button<LightProps> name={"Brian Auto"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                        <Button<LightProps> name={"Brian On"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"default"} priority={100} />
                        <Button<SwitchProps> name={"Brian Fan"} topic_substr={"Brian/Fan"} action={Action::Toggle} icon={fan_icon} />
                        <Button<MusicProps> name={"Sleep"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon.clone()} play_list={"sleep"} />
                        <Button<MusicProps> name={"Wakeup"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon} play_list={"wake_up"} />
                    </div>
                    <div class="flex flex-row">
                        <h2>{"Passage"}</h2>
                        <Button<LightProps> name={"Passage Auto"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                        <Button<LightProps> name={"Passage On"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon} scene={"default"} priority={100} />
                    </div>
                    <div class="flex flex-row">

                    <h2>{"Dining Room"}</h2>
                        <Button<SwitchProps> name={"TV"} topic_substr={"Dining/TvSwitch"} action={Action::Toggle} icon={tv_icon} />
                    </div>
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
