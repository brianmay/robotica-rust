use yew::prelude::*;

use robotica_frontend::components::button::{Button, LightProps, MusicProps, SwitchProps};
use robotica_frontend::services::controllers::{Action, Icon};

use super::require_connection::RequireConnection;

#[function_component(Lights)]
pub fn lights_view() -> Html {
    let light_icon = Icon::new("light");
    let wake_up_icon = Icon::new("wake_up");
    let fan_icon = Icon::new("fan");
    let tv_icon = Icon::new("tv");

    html!(
        <RequireConnection>
            <h2>{"Brian's Room"}</h2>
            <div class="buttons">
                <Button<LightProps> name={"Brian Auto"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                <Button<LightProps> name={"Brian On"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"default"} priority={100} />
                <Button<SwitchProps> name={"Brian Fan"} topic_substr={"Brian/Fan"} action={Action::Toggle} icon={fan_icon} />
                <Button<MusicProps> name={"Sleep"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon.clone()} play_list={"sleep"} />
                <Button<MusicProps> name={"Wakeup"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={wake_up_icon} play_list={"wake_up"} />
            </div>

            <h2>{"Passage"}</h2>
            <div class="buttons">
                <Button<LightProps> name={"Passage Auto"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                <Button<LightProps> name={"Passage On"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon} scene={"default"} priority={100} />
            </div>

            <h2>{"Dining Room"}</h2>
            <div class="buttons">
                <Button<SwitchProps> name={"TV"} topic_substr={"Dining/TvSwitch"} action={Action::Toggle} icon={tv_icon} />
            </div>
        </RequireConnection>
    )
}
