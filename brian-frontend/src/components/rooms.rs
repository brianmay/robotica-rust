use robotica_frontend::services::icons::Icon;
use yew::prelude::*;

use robotica_frontend::components::button::{
    Button, HdmiProps, Light2Props, LightProps, MusicProps, SwitchProps, TasmotaProps,
};
use robotica_frontend::components::mqtt_last::MqttLast;

use robotica_common::anavi_thermometer as anavi;
use robotica_common::controllers::Action;
use robotica_common::zigbee2mqtt;

use super::require_connection::RequireConnection;

#[function_component(BrianRoom)]
pub fn brian_room() -> Html {
    let light_icon = Icon::new("light");
    let speaker_icon = Icon::new("speaker");
    let fan_icon = Icon::new("fan");
    let night_icon = Icon::new("night");
    let trumpet_icon = Icon::new("trumpet");

    html!(
        <RequireConnection>
            <h1>{ "Brian's Room" }</h1>

            <h2>
                {"Lights"}
            </h2>
            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} />
                <Button<Light2Props> name={"On"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"rainbow"} />
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
                <Button<SwitchProps> name={"Night"} topic_substr={"Brian/Night"} action={Action::Toggle} icon={night_icon} />
                <Button<SwitchProps> name={"MSG"} topic_substr={"Brian/Messages"} action={Action::Toggle} icon={trumpet_icon} />
            </div>
        </RequireConnection>
    )
}

#[function_component(JanRoom)]
pub fn jan_room() -> Html {
    let light_icon = Icon::new("light");
    let speaker_icon = Icon::new("speaker");

    html!(
        <RequireConnection>
            <h1>{ "Jan's Room" }</h1>

            <h2>
                {"Lights - "}
                <MqttLast<String> topic="state/jan/Light/power"/>
            </h2>
            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Jan/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} />
                <Button<Light2Props> name={"On"} topic_substr={"Jan/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Jan/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"rainbow"} />
            </div>

            <h2>{"Music"}</h2>
            <div class="buttons">
                <Button<MusicProps> name={"Stargate"} topic_substr={"Jan/Robotica"} action={Action::Toggle} icon={speaker_icon.clone()} play_list={"stargate"} />
                <Button<MusicProps> name={"Frozen"} topic_substr={"Jan/Robotica"} action={Action::Toggle} icon={speaker_icon.clone()} play_list={"frozen"} />
                <Button<MusicProps> name={"Dragon"} topic_substr={"Jan/Robotica"} action={Action::Toggle} icon={speaker_icon.clone()} play_list={"train_dragon"} />
                <Button<MusicProps> name={"Wakeup"} topic_substr={"Jan/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"wake_up"} />
            </div>
        </RequireConnection>
    )
}

#[function_component(TwinsRoom)]
pub fn twins_room() -> Html {
    let light_icon = Icon::new("light");
    let speaker_icon = Icon::new("speaker");

    html!(
        <RequireConnection>
            <h1>{ "Twins' Room" }</h1>

            <h2>
                {"Lights - "}
                <MqttLast<String> topic="state/Twins/Light/power"/>
            </h2>
            <div class="buttons">
                <Button<LightProps> name={"Auto"} topic_substr={"Twins/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                <Button<LightProps> name={"On"} topic_substr={"Twins/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"default"} priority={100} />
                <Button<LightProps> name={"Rainbow"} topic_substr={"Twins/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"rainbow"} priority={100} />
                <Button<LightProps> name={"Declan"} topic_substr={"Twins/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"night_declan"} priority={100} />
                <Button<LightProps> name={"Nikolai"} topic_substr={"Twins/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"night_nikolai"} priority={100} />
            </div>

            <h2>{"Music"}</h2>
            <div class="buttons">
                <Button<MusicProps> name={"Stargate"} topic_substr={"Twins/Robotica"} action={Action::Toggle} icon={speaker_icon.clone()} play_list={"stargate"} />
                <Button<MusicProps> name={"Star Trek"} topic_substr={"Twins/Robotica"} action={Action::Toggle} icon={speaker_icon.clone()} play_list={"startrek"} />
                <Button<MusicProps> name={"Doom"} topic_substr={"Twins/Robotica"} action={Action::Toggle} icon={speaker_icon.clone()} play_list={"dragons_doom"} />
                <Button<MusicProps> name={"Dragon"} topic_substr={"Twins/Robotica"} action={Action::Toggle} icon={speaker_icon.clone()} play_list={"train_dragon"} />
                <Button<MusicProps> name={"Wakeup"} topic_substr={"Twins/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"wake_up"} />
            </div>
        </RequireConnection>
    )
}

#[function_component(ColinRoom)]
pub fn colin_room() -> Html {
    let select_icon = Icon::new("select");

    html!(
        <RequireConnection>
            <h1>{ "Colin's Room" }</h1>

            <h2>{"TV"}</h2>
            <div class="buttons">
                <Button<HdmiProps> name={"WiiU"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon.clone()} input=1 output=3 />
                <Button<HdmiProps> name={"Google"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon.clone()} input=2 output=3 />
                <Button<HdmiProps> name={"Xbox"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon.clone()} input=3 output=3 />
                <Button<HdmiProps> name={"MythTV"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=4 output=3 />
            </div>
        </RequireConnection>
    )
}

#[function_component(AkiraRoom)]
pub fn akira_room() -> Html {
    let light_icon = Icon::new("light");
    let speaker_icon = Icon::new("speaker");

    html!(
        <RequireConnection>
            <h1>{ "Akira's Room" }</h1>

            <h2>
                {"Lights - "}
                <MqttLast<String> topic="state/Akira/Light/power"/>
            </h2>
            <div class="buttons">
                <Button<LightProps> name={"Auto"} topic_substr={"Akira/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} priority={100} />
                <Button<LightProps> name={"On"} topic_substr={"Akira/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"default"} priority={100} />
                <Button<LightProps> name={"Rainbow"} topic_substr={"Akira/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"rainbow"} priority={100} />
                <Button<LightProps> name={"Night"} topic_substr={"Akira/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"night_akira"} priority={100} />
            </div>

            <h2>{"Music"}</h2>
            <div class="buttons">
                <Button<MusicProps> name={"Stargate"} topic_substr={"Akira/Robotica"} action={Action::Toggle} icon={speaker_icon.clone()} play_list={"stargate"} />
                <Button<MusicProps> name={"Frozen"} topic_substr={"Akira/Robotica"} action={Action::Toggle} icon={speaker_icon.clone()} play_list={"frozen"} />
                <Button<MusicProps> name={"Dragon"} topic_substr={"Akira/Robotica"} action={Action::Toggle} icon={speaker_icon.clone()} play_list={"train_dragon"} />
                <Button<MusicProps> name={"Wakeup"} topic_substr={"Akira/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"wake_up"} />
            </div>
        </RequireConnection>
    )
}

#[function_component(DiningRoom)]
pub fn dining_room() -> Html {
    let light_icon = Icon::new("light");
    let tv_icon = Icon::new("tv");
    let select_icon = Icon::new("select");
    let trumpet_icon = Icon::new("trumpet");

    html!(
        <RequireConnection>
            <h1>{ "Dining Room" }</h1>

            <div>
                {"Front door - "} <MqttLast<zigbee2mqtt::Door> topic="zigbee2mqtt/Dining/door"/>
            </div>

            <h2>{"Switches"}</h2>
            <div class="buttons">
                <Button<TasmotaProps> name={"TV"} topic_substr={"tasmota_31E56F"} action={Action::Toggle} icon={tv_icon} />
                <Button<SwitchProps> name={"MSG"} topic_substr={"Dining/Messages"} action={Action::Toggle} icon={trumpet_icon} />
            </div>

            <h2>{"TV"}</h2>
            <div class="buttons">
                <Button<HdmiProps> name={"WiiU"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon.clone()} input=1 output=1 />
                <Button<HdmiProps> name={"Google"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon.clone()} input=2 output=1 />
                <Button<HdmiProps> name={"Xbox"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon.clone()} input=3 output=1 />
                <Button<HdmiProps> name={"MythTV"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=4 output=1 />
            </div>

            <h2>
                {"Lights - "}
                <MqttLast<String> topic="state/Dining/Light/power"/>
            </h2>
            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Dining/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"} />
                <Button<Light2Props> name={"On"} topic_substr={"Dining/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Dining/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"rainbow"} />
            </div>
        </RequireConnection>
    )
}

#[function_component(LoungeRoom)]
pub fn lounge_room() -> Html {
    let select_icon = Icon::new("select");

    html!(
        <RequireConnection>
            <h1>{ "Lounge Room" }</h1>

            <h2>{"TV"}</h2>
            <div class="buttons">
                <Button<HdmiProps> name={"WiiU"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon.clone()} input=1 output=2 />
                <Button<HdmiProps> name={"Google"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon.clone()} input=2 output=2 />
                <Button<HdmiProps> name={"Xbox"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon.clone()} input=3 output=2 />
                <Button<HdmiProps> name={"MythTV"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=4 output=2 />
            </div>
        </RequireConnection>
    )
}

#[function_component(Bathroom)]
pub fn bathroom() -> Html {
    let schedule_icon = Icon::new("schedule");

    html!(
        <RequireConnection>
            <h1>{ "Bathroom" }</h1>

            <table class="table container table-striped table-hover">
                <tbody>
                    <tr>
                        <th>{"Bathroom door"}</th>
                        <td>
                            <MqttLast<zigbee2mqtt::Door> topic="zigbee2mqtt/Bathroom/door"/>
                        </td>
                    </tr>

                    <tr>
                        <th>{"Air temperature"}</th>
                        <td>
                            <MqttLast<anavi::Temperature> topic="workgroup/3765653003a76f301ad767b4676d7065/air/temperature"/>
                        </td>
                    </tr>

                    <tr>
                        <th>{"Air humidity"}</th>
                        <td>
                            <MqttLast<anavi::Humidity> topic="workgroup/3765653003a76f301ad767b4676d7065/air/humidity"/>
                        </td>
                    </tr>

                    <tr>
                        <th>{"Water temperature"}</th>
                        <td>
                            <MqttLast<anavi::Temperature> topic="workgroup/3765653003a76f301ad767b4676d7065/water/temperature"/>
                        </td>
                    </tr>
                </tbody>
            </table>

            <h2>{"Switches"}</h2>
            <div class="buttons">
                <Button<SwitchProps> name={"Brian"} topic_substr={"Brian/Request_Bathroom"} action={Action::Toggle} icon={schedule_icon.clone()} />
                <Button<SwitchProps> name={"Dining"} topic_substr={"Dining/Request_Bathroom"} action={Action::Toggle} icon={schedule_icon} />
            </div>
        </RequireConnection>
    )
}

#[function_component(Passage)]
pub fn passage() -> Html {
    let light_icon = Icon::new("light");

    html!(
        <RequireConnection>
            <h1>{"Passage"}</h1>

            <h2>
                {"Lights"}
            </h2>

            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"}  />
                <Button<Light2Props> name={"On"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon.clone()} scene={"rainbow"}  />
            </div>

            <h2>
                {"Lights - Cupboard"}
            </h2>

            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Passage/Light/split/cupboard"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"}  />
                <Button<Light2Props> name={"On"} topic_substr={"Passage/Light/split/cupboard"} action={Action::Toggle} icon={light_icon.clone()} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Passage/Light/split/cupboard"} action={Action::Toggle} icon={light_icon.clone()} scene={"rainbow"}  />
                <Button<Light2Props> name={"Busy"} topic_substr={"Passage/Light/split/cupboard"} action={Action::Toggle} icon={light_icon.clone()} scene={"busy"}  />
            </div>

            <h2>
                {"Lights - Bathroom"}
            </h2>

            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Passage/Light/split/bathroom"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"}  />
                <Button<Light2Props> name={"On"} topic_substr={"Passage/Light/split/bathroom"} action={Action::Toggle} icon={light_icon.clone()} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Passage/Light/split/bathroom"} action={Action::Toggle} icon={light_icon.clone()} scene={"rainbow"}  />
                <Button<Light2Props> name={"Busy"} topic_substr={"Passage/Light/split/bathroom"} action={Action::Toggle} icon={light_icon.clone()} scene={"busy"}  />
            </div>

            <h2>
            {"Lights - Bedroom"}
            </h2>

            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Passage/Light/split/bedroom"} action={Action::Toggle} icon={light_icon.clone()} scene={"auto"}  />
                <Button<Light2Props> name={"On"} topic_substr={"Passage/Light/split/bedroom"} action={Action::Toggle} icon={light_icon.clone()} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Passage/Light/split/bedroom"} action={Action::Toggle} icon={light_icon.clone()} scene={"rainbow"}  />
                <Button<Light2Props> name={"Busy"} topic_substr={"Passage/Light/split/bedroom"} action={Action::Toggle} icon={light_icon.clone()} scene={"busy"}  />
            </div>

        </RequireConnection>
    )
}

#[function_component(Tesla)]
pub fn tesla() -> Html {
    let light_icon = Icon::new("light");

    html!(
        <RequireConnection>
            <h1>{ "Tesla" }</h1>

            <h2>{"Switches"}</h2>
            <div class="buttons">
                <Button<SwitchProps> name={"Charge"} topic_substr={"Tesla/1/AutoCharge"} action={Action::Toggle} icon={light_icon.clone()} />
                <Button<SwitchProps> name={"Force"} topic_substr={"Tesla/1/ForceCharge"} action={Action::Toggle} icon={light_icon} />
            </div>
        </RequireConnection>
    )
}
