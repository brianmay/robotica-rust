use yew::prelude::*;

use robotica_frontend::components::button::{Button, LightProps, MusicProps, SwitchProps};
use robotica_frontend::components::mqtt_last::MqttLast;
use robotica_frontend::services::controllers::{Action, Icon};

use crate::zigbee2mqtt;

use super::require_connection::RequireConnection;

#[function_component(BrianRoom)]
pub fn brian_room() -> Html {
    let light_icon = Icon::new("light");
    let speaker_icon = Icon::new("speaker");
    let fan_icon = Icon::new("fan");

    html!(
        <RequireConnection>
            <h1>{ "Brian's Room" }</h1>

            <h2>
                {"Lights - "}
                <MqttLast<String> topic="state/Brian/Light/power"/>
            </h2>
            <div class="buttons">
                <Button<LightProps> name={"Auto"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                <Button<LightProps> name={"On"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"default"} priority={100} />
                <Button<LightProps> name={"Rainbow"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"rainbow"} priority={100} />
            </div>

            <h2>{"Music"}</h2>
            <div class="buttons">
                <Button<MusicProps> name={"Frozen"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={speaker_icon.clone()} play_list={"frozen"} />
                <Button<MusicProps> name={"Sleep"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={speaker_icon.clone()} play_list={"sleep"} />
                <Button<MusicProps> name={"Wakeup"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"wake_up"} />
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

            <div>
                {"Front door - "} <MqttLast<zigbee2mqtt::Door> topic="zigbee2mqtt/Dining/door"/>
            </div>

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

#[function_component(TwinsRoom)]
pub fn twins_room() -> Html {
    html!(
        <RequireConnection>
            <h1>{ "Twins Room" }</h1>
        </RequireConnection>
    )
}

#[function_component(Bathroom)]
pub fn bathroom() -> Html {
    html!(
        <RequireConnection>
            <h1>{ "Bathroom" }</h1>

            <div>
                {"Bathroom door - "} <MqttLast<zigbee2mqtt::Door> topic="zigbee2mqtt/Bathroom/door"/>
            </div>
        </RequireConnection>
    )
}
