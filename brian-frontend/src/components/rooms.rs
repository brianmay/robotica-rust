use std::ops::Deref;

use robotica_common::websocket::MqttMessage;
use robotica_frontend::services::websocket::event_bus::{Command, EventBus};
use yew::prelude::*;

use robotica_frontend::components::button::{Button, LightProps, MusicProps, SwitchProps};
use robotica_frontend::services::controllers::{Action, Icon};
use yew_agent::Bridged;

use super::require_connection::RequireConnection;

#[function_component(BrianRoom)]
pub fn brian_room() -> Html {
    let callback = { Callback::from(move |_| {}) };

    let events = use_mut_ref(|| EventBus::bridge(callback));

    let light_power = use_state(|| None);

    let callback = {
        let light_power = light_power.clone();
        Callback::from(move |msg: MqttMessage| {
            light_power.set(Some(msg.payload));
        })
    };

    use_ref(|| {
        let topic = "state/Brian/Light/power".to_string();
        let subscribe = Command::Subscribe { topic, callback };
        events.borrow_mut().send(subscribe);
    });

    let light_icon = Icon::new("light");
    let wake_up_icon = Icon::new("wake_up");
    let fan_icon = Icon::new("fan");

    html!(
        <RequireConnection>
            <h1>{ "Brian's Room" }</h1>

            <h2>
                {"Lights"}
                if let Some(power) = light_power.deref() {
                    {" - "} {power}
                }
            </h2>
            <div class="buttons">
                <Button<LightProps> name={"Auto"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                <Button<LightProps> name={"On"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"default"} priority={100} />
                <Button<LightProps> name={"Rainbow"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"rainbow"} priority={100} />
            </div>

            <h2>{"Music"}</h2>
            <div class="buttons">
                <Button<MusicProps> name={"Frozen"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon.clone()} play_list={"frozen"} />
                <Button<MusicProps> name={"Sleep"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon.clone()} play_list={"sleep"} />
                <Button<MusicProps> name={"Wakeup"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon} play_list={"wake_up"} />
            </div>

            <h2>{"Switches"}</h2>
            <div class="buttons">
                <Button<SwitchProps> name={"Fan"} topic_substr={"Brian/Fan"} action={Action::Toggle} icon={fan_icon} />
            </div>

            <h2>{"Passage"}</h2>
            <div class="buttons">
                <Button<LightProps> name={"Auto"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                <Button<LightProps> name={"On"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon} scene={"default"} priority={100} />
            </div>
        </RequireConnection>
    )
}

#[function_component(DiningRoom)]
pub fn dining_room() -> Html {
    let light_icon = Icon::new("light");
    let tv_icon = Icon::new("tv");

    html!(
        <RequireConnection>
            <h1>{ "Dining Room" }</h1>

            <h2>{"Switches"}</h2>
            <div class="buttons">
                <Button<SwitchProps> name={"TV"} topic_substr={"Dining/TvSwitch"} action={Action::Toggle} icon={tv_icon} />
            </div>

            <h2>{"Passage"}</h2>
            <div class="buttons">
                <Button<LightProps> name={"Auto"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                <Button<LightProps> name={"On"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon} scene={"default"} priority={100} />
            </div>
        </RequireConnection>
    )
}
