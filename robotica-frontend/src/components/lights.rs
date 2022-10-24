use log::info;
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

    let connected = use_state(|| false);

    let callback = {
        let connected = connected.clone();

        Callback::from(move |msg: WsEvent| match msg {
            WsEvent::Connect => {
                connected.set(true);
            }
            WsEvent::Disconnect => {
                connected.set(false);
            }
        })
    };

    let wss = use_context::<WebsocketService>().expect("No context found.");
    use_ref(|| {
        info!("Connecting to event handler");
        let msg = Command::EventHandler(callback);
        let mut tx = wss.tx;
        tx.try_send(msg).unwrap();
    });

    html! {
        <>
            if !*connected {
                <div class="alert alert-danger" role="alert">
                    { "The connection is down." }
                </div>
            }
            <div>
                <h2>{"Brian's Room"}</h2>
                <Button<LightProps> name={"Brian Auto"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                <Button<LightProps> name={"Brian On"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"default"} priority={100} />
                <Button<SwitchProps> name={"Brian Fan"} topic_substr={"Brian/Fan"} action={Action::Toggle} icon={fan_icon} />
                <Button<MusicProps> name={"Sleep"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon.clone()} play_list={"sleep"} />
                <Button<MusicProps> name={"Wakeup"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon} play_list={"wake_up"} />
            </div>
            <div>
                <h2>{"Passage"}</h2>
                <Button<LightProps> name={"Passage Auto"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                <Button<LightProps> name={"Passage On"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon} scene={"default"} priority={100} />
            </div>
            <div>
                <h2>{"Dining Room"}</h2>
                <Button<SwitchProps> name={"TV"} topic_substr={"Dining/TvSwitch"} action={Action::Toggle} icon={tv_icon} />
            </div>

        </>
    }
}
