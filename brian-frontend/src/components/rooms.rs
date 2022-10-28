use yew::prelude::*;

use robotica_frontend::components::button::{Button, LightProps, MusicProps, SwitchProps};
use robotica_frontend::components::ws_client::WsClient;
use robotica_frontend::services::controllers::{Action, Icon};

use super::require_connection::RequireConnection;

#[function_component(BrianRoom)]
pub fn brian_room() -> Html {
    let light_icon = Icon::new("light");
    let wake_up_icon = Icon::new("wake_up");
    let fan_icon = Icon::new("fan");

    html!(
        <WsClient>
            <RequireConnection>
                <h1>{ "Brian's Room" }</h1>

                <h2>{"Lights"}</h2>
                <div class="buttons">
                    <Button<LightProps> name={"Brian Auto"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                    <Button<LightProps> name={"Brian On"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"default"} priority={100} />
                </div>

                <h2>{"Music"}</h2>
                <div class="buttons">
                    <Button<MusicProps> name={"Frozen"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon.clone()} play_list={"frozen"} />
                    <Button<MusicProps> name={"Sleep"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon.clone()} play_list={"sleep"} />
                    <Button<MusicProps> name={"Wakeup"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon} play_list={"wake_up"} />
                </div>

                <h2>{"Switches"}</h2>
                <div class="buttons">
                    <Button<SwitchProps> name={"Brian Fan"} topic_substr={"Brian/Fan"} action={Action::Toggle} icon={fan_icon} />
                </div>

                <h2>{"Passage"}</h2>
                <div class="buttons">
                    <Button<LightProps> name={"Passage Auto"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                    <Button<LightProps> name={"Passage On"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon} scene={"default"} priority={100} />
                </div>
            </RequireConnection>
        </WsClient>
    )
}

#[function_component(DiningRoom)]
pub fn dining_room() -> Html {
    let light_icon = Icon::new("light");
    let tv_icon = Icon::new("tv");

    html!(
        <WsClient>
            <RequireConnection>
                <h1>{ "Dining Room" }</h1>

                <h2>{"Switches"}</h2>
                <div class="buttons">
                    <Button<SwitchProps> name={"TV"} topic_substr={"Dining/TvSwitch"} action={Action::Toggle} icon={tv_icon} />
                </div>

                <h2>{"Passage"}</h2>
                <div class="buttons">
                    <Button<LightProps> name={"Passage Auto"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                    <Button<LightProps> name={"Passage On"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon} scene={"default"} priority={100} />
                </div>
            </RequireConnection>
        </WsClient>
    )
}
