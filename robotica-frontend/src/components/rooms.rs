use std::sync::Arc;

use yew::prelude::*;

use crate::components::button::{
    Button, HdmiProps, Light2Props, Music2Props, SwitchProps, TasmotaProps, ZwaveProps,
};
use crate::components::mqtt_last::MqttLast;

use robotica_common::anavi_thermometer as anavi;
use robotica_common::config::{
    ButtonConfig, ButtonRowConfig, ControllerConfig, Icon, RoomConfig, Rooms,
};
use robotica_common::controllers::Action;
use robotica_common::mqtt::Json;
use robotica_common::zigbee2mqtt;

use super::require_connection::RequireConnection;

fn controller_to_html(title: String, icon: Icon, controller_config: &ControllerConfig) -> Html {
    match controller_config {
        ControllerConfig::Hdmi(config) => {
            let props = HdmiProps {
                name: title,
                icon,
                action: config.action,
                topic_substr: config.topic_substr.clone(),
                input: config.input,
                output: config.output,
            };
            html! { <Button<HdmiProps> ..props /> }
        }
        ControllerConfig::Light2(config) => {
            let props = Light2Props {
                name: title,
                icon,
                action: config.action,
                topic_substr: config.topic_substr.clone(),
                scene: config.scene.clone(),
            };
            html! { <Button<Light2Props> ..props /> }
        }
        ControllerConfig::Music2(config) => {
            let props = Music2Props {
                name: title,
                icon,
                action: config.action,
                topic_substr: config.topic_substr.clone(),
                play_list: config.play_list.clone(),
            };
            html! { <Button<Music2Props> ..props /> }
        }
        ControllerConfig::Switch(config) => {
            let props = SwitchProps {
                name: title,
                icon,
                action: config.action,
                topic_substr: config.topic_substr.clone(),
            };
            html! { <Button<SwitchProps> ..props /> }
        }
        ControllerConfig::Zwave(config) => {
            let props = ZwaveProps {
                name: title,
                icon,
                action: config.action,
                topic_substr: config.topic_substr.clone(),
            };
            html! { <Button<ZwaveProps> ..props /> }
        }
        ControllerConfig::Tasmota(config) => {
            let props = TasmotaProps {
                name: title,
                icon,
                action: Action::Toggle,
                power_postfix: config.power_postfix.clone(),
                topic_substr: config.topic_substr.clone(),
            };
            html! { <Button<TasmotaProps> ..props /> }
        }
    }
}

fn button_to_html(button: &ButtonConfig) -> Html {
    let icon = button.icon;
    let title = button.title.clone();

    html!(
        <span key={button.id.clone()}>
            { controller_to_html(title, icon, &button.controller) }
        </span>
    )
}

fn rows_to_html(rows: &[ButtonRowConfig]) -> Html {
    html!(
        <div>
            {
            rows.iter().map(|row| {
                html!(
                    <div>
                        <h2 key={row.id.clone()}>{row.title.clone()}</h2>
                        <div class="buttons">
                            {row.buttons.iter().map(button_to_html).collect::<Html>()}
                        </div>
                    </div>
                )
            }).collect::<Html>()
            }
        </div>
    )
}

fn room_to_html(room: &RoomConfig) -> Html {
    html!(
        <div>
            <h1>{room.title.clone()}</h1>
            { rows_to_html(&room.rows) }
        </div>
    )
}

#[derive(Properties, Eq, PartialEq)]
pub struct Props {
    pub id: String,
}

#[function_component(Room)]
pub fn room(props: &Props) -> Html {
    let rooms = use_context::<Option<Arc<Rooms>>>().unwrap();

    if let Some(rooms) = rooms {
        if let Some(room) = rooms.iter().find(|room| room.id == props.id) {
            html!(
                <RequireConnection>
                { room_to_html(room) }
                </RequireConnection>
            )
        } else {
            html!(<h1>{"404 Please ask a Penguin for help"}</h1>)
        }
    } else {
        html!(<h1>{"404 Please ask a Penguin for help"}</h1>)
    }
}

#[function_component(BrianRoom)]
pub fn brian_room() -> Html {
    let light_icon = Icon::Light;
    let speaker_icon = Icon::Speaker;
    let fan_icon = Icon::Fan;
    let night_icon = Icon::Light;
    let trumpet_icon = Icon::Trumpet;

    html!(
        <RequireConnection>
            <h1>{ "Brian's Room" }</h1>

            <h2>
                {"Lights"}
            </h2>
            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon} scene={"auto"} />
                <Button<Light2Props> name={"On"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Brian/Light"} action={Action::Toggle} icon={light_icon} scene={"rainbow"} />
            </div>

            <h2>{"Music"}</h2>
            <div class="buttons">
                <Button<Music2Props> name={"Frozen"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"frozen"} />
                <Button<Music2Props> name={"Sleep"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"sleep"} />
                <Button<Music2Props> name={"Wakeup"} topic_substr={"Brian/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"wake_up"} />
            </div>

            <h2>{"Switches"}</h2>
            <div class="buttons">
                <Button<ZwaveProps> name={"Fan"} topic_substr={"Brians_Bedroom/Fan"} action={Action::Toggle} icon={fan_icon} />
                <Button<SwitchProps> name={"Night"} topic_substr={"Brian/Night"} action={Action::Toggle} icon={night_icon} />
                <Button<SwitchProps> name={"MSG"} topic_substr={"Brian/Messages"} action={Action::Toggle} icon={trumpet_icon} />
            </div>
        </RequireConnection>
    )
}

#[function_component(JanRoom)]
pub fn jan_room() -> Html {
    let light_icon = Icon::Light;
    let speaker_icon = Icon::Speaker;

    html!(
        <RequireConnection>
            <h1>{ "Jan's Room" }</h1>

            <h2>
                {"Lights"}
            </h2>
            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Jan/Light"} action={Action::Toggle} icon={light_icon} scene={"auto"} />
                <Button<Light2Props> name={"On"} topic_substr={"Jan/Light"} action={Action::Toggle} icon={light_icon} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Jan/Light"} action={Action::Toggle} icon={light_icon} scene={"rainbow"} />
            </div>

            <h2>{"Music"}</h2>
            <div class="buttons">
                <Button<Music2Props> name={"Stargate"} topic_substr={"Jan/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"stargate"} />
                <Button<Music2Props> name={"Frozen"} topic_substr={"Jan/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"frozen"} />
                <Button<Music2Props> name={"Dragon"} topic_substr={"Jan/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"train_dragon"} />
                <Button<Music2Props> name={"Wakeup"} topic_substr={"Jan/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"wake_up"} />
            </div>
        </RequireConnection>
    )
}

#[function_component(TwinsRoom)]
pub fn twins_room() -> Html {
    let light_icon = Icon::Light;
    let speaker_icon = Icon::Speaker;

    html!(
        <RequireConnection>
            <h1>{ "Twins' Room" }</h1>

            <h2>
                {"Lights"}
            </h2>
            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Twins/Light"} action={Action::Toggle} icon={light_icon} scene={"auto"} />
                <Button<Light2Props> name={"On"} topic_substr={"Twins/Light"} action={Action::Toggle} icon={light_icon} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Twins/Light"} action={Action::Toggle} icon={light_icon} scene={"rainbow"} />
                <Button<Light2Props> name={"Declan"} topic_substr={"Twins/Light"} action={Action::Toggle} icon={light_icon} scene={"declan-night"}  />
                <Button<Light2Props> name={"Nikolai"} topic_substr={"Twins/Light"} action={Action::Toggle} icon={light_icon} scene={"nikolai-night"} />
            </div>

            <h2>{"Music"}</h2>
            <div class="buttons">
                <Button<Music2Props> name={"Stargate"} topic_substr={"Twins/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"stargate"} />
                <Button<Music2Props> name={"Star Trek"} topic_substr={"Twins/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"startrek"} />
                <Button<Music2Props> name={"Doom"} topic_substr={"Twins/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"dragons_doom"} />
                <Button<Music2Props> name={"Dragon"} topic_substr={"Twins/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"train_dragon"} />
                <Button<Music2Props> name={"Wakeup"} topic_substr={"Twins/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"twins_wake_up"} />
            </div>
        </RequireConnection>
    )
}

#[function_component(ColinRoom)]
pub fn colin_room() -> Html {
    let select_icon = Icon::Select;

    html!(
        <RequireConnection>
            <h1>{ "Colin's Room" }</h1>

            <h2>{"TV"}</h2>
            <div class="buttons">
                <Button<HdmiProps> name={"WiiU"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=1 output=3 />
                <Button<HdmiProps> name={"Google"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=2 output=3 />
                <Button<HdmiProps> name={"Xbox"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=3 output=3 />
                <Button<HdmiProps> name={"MythTV"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=4 output=3 />
            </div>
        </RequireConnection>
    )
}

#[function_component(AkiraRoom)]
pub fn akira_room() -> Html {
    let light_icon = Icon::Light;
    let speaker_icon = Icon::Speaker;

    html!(
        <RequireConnection>
            <h1>{ "Akira's Room" }</h1>

            <h2>
                {"Lights"}
            </h2>
            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Akira/Light"} action={Action::Toggle} icon={light_icon} scene={"auto"} />
                <Button<Light2Props> name={"On"} topic_substr={"Akira/Light"} action={Action::Toggle} icon={light_icon} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Akira/Light"} action={Action::Toggle} icon={light_icon} scene={"rainbow"} />
                <Button<Light2Props> name={"Night"} topic_substr={"Akira/Light"} action={Action::Toggle} icon={light_icon} scene={"akira-night"} />
            </div>

            <h2>{"Music"}</h2>
            <div class="buttons">
                <Button<Music2Props> name={"Stargate"} topic_substr={"Akira/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"stargate"} />
                <Button<Music2Props> name={"Frozen"} topic_substr={"Akira/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"frozen"} />
                <Button<Music2Props> name={"Dragon"} topic_substr={"Akira/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"train_dragon"} />
                <Button<Music2Props> name={"Wakeup"} topic_substr={"Akira/Robotica"} action={Action::Toggle} icon={speaker_icon} play_list={"wake_up"} />
            </div>
        </RequireConnection>
    )
}

#[function_component(DiningRoom)]
pub fn dining_room() -> Html {
    let light_icon = Icon::Light;
    let tv_icon = Icon::Tv;
    let select_icon = Icon::Select;
    let trumpet_icon = Icon::Trumpet;

    html!(
        <RequireConnection>
            <h1>{ "Dining Room" }</h1>

            <div>
                {"Front door - "} <MqttLast<Json<zigbee2mqtt::Door>> topic="zigbee2mqtt/Dining/door"/>
            </div>

            <h2>{"Switches"}</h2>
            <div class="buttons">
                <Button<TasmotaProps> name={"TV"} topic_substr={"tasmota_31E56F"} action={Action::Toggle} icon={tv_icon} />
                <Button<SwitchProps> name={"MSG"} topic_substr={"Dining/Messages"} action={Action::Toggle} icon={trumpet_icon} />
            </div>

            <h2>{"TV"}</h2>
            <div class="buttons">
                <Button<HdmiProps> name={"WiiU"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=1 output=1 />
                <Button<HdmiProps> name={"Google"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=2 output=1 />
                <Button<HdmiProps> name={"Xbox"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=3 output=1 />
                <Button<HdmiProps> name={"MythTV"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=4 output=1 />
            </div>

            <h2>
                {"Lights"}
            </h2>
            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Dining/Light"} action={Action::Toggle} icon={light_icon} scene={"auto"} />
                <Button<Light2Props> name={"On"} topic_substr={"Dining/Light"} action={Action::Toggle} icon={light_icon} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Dining/Light"} action={Action::Toggle} icon={light_icon} scene={"rainbow"} />
            </div>
        </RequireConnection>
    )
}

#[function_component(LoungeRoom)]
pub fn lounge_room() -> Html {
    let select_icon = Icon::Select;

    html!(
        <RequireConnection>
            <h1>{ "Lounge Room" }</h1>

            <h2>{"TV"}</h2>
            <div class="buttons">
                <Button<HdmiProps> name={"WiiU"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=1 output=2 />
                <Button<HdmiProps> name={"Google"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=2 output=2 />
                <Button<HdmiProps> name={"Xbox"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=3 output=2 />
                <Button<HdmiProps> name={"MythTV"} topic_substr={"Dining/TV"} action={Action::Toggle} icon={select_icon} input=4 output=2 />
            </div>
        </RequireConnection>
    )
}

#[function_component(Bathroom)]
pub fn bathroom() -> Html {
    let schedule_icon = Icon::Schedule;

    html!(
        <RequireConnection>
            <h1>{ "Bathroom" }</h1>

            <table class="table container table-striped table-hover">
                <tbody>
                    <tr>
                        <th>{"Bathroom door"}</th>
                        <td>
                            <MqttLast<Json<zigbee2mqtt::Door>> topic="zigbee2mqtt/Bathroom/door"/>
                        </td>
                    </tr>

                    <tr>
                        <th>{"Air temperature"}</th>
                        <td>
                            <MqttLast<Json<anavi::Temperature>> topic="workgroup/3765653003a76f301ad767b4676d7065/air/temperature"/>
                        </td>
                    </tr>

                    <tr>
                        <th>{"Air humidity"}</th>
                        <td>
                            <MqttLast<Json<anavi::Humidity>> topic="workgroup/3765653003a76f301ad767b4676d7065/air/humidity"/>
                        </td>
                    </tr>

                    <tr>
                        <th>{"Water temperature"}</th>
                        <td>
                            <MqttLast<Json<anavi::Temperature>> topic="workgroup/3765653003a76f301ad767b4676d7065/water/temperature"/>
                        </td>
                    </tr>
                </tbody>
            </table>

            <h2>{"Switches"}</h2>
            <div class="buttons">
                <Button<SwitchProps> name={"Brian"} topic_substr={"Brian/Request_Bathroom"} action={Action::Toggle} icon={schedule_icon} />
                <Button<SwitchProps> name={"Dining"} topic_substr={"Dining/Request_Bathroom"} action={Action::Toggle} icon={schedule_icon} />
            </div>
        </RequireConnection>
    )
}

#[function_component(Passage)]
pub fn passage() -> Html {
    let light_icon = Icon::Light;

    html!(
        <RequireConnection>
            <h1>{"Passage"}</h1>

            <h2>
                {"Lights"}
            </h2>

            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon} scene={"auto"}  />
                <Button<Light2Props> name={"On"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Passage/Light"} action={Action::Toggle} icon={light_icon} scene={"rainbow"}  />
            </div>

            <h2>
                {"Lights - Cupboard"}
            </h2>

            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Passage/Light/split/cupboard"} action={Action::Toggle} icon={light_icon} scene={"auto"}  />
                <Button<Light2Props> name={"On"} topic_substr={"Passage/Light/split/cupboard"} action={Action::Toggle} icon={light_icon} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Passage/Light/split/cupboard"} action={Action::Toggle} icon={light_icon} scene={"rainbow"}  />
                <Button<Light2Props> name={"Busy"} topic_substr={"Passage/Light/split/cupboard"} action={Action::Toggle} icon={light_icon} scene={"busy"}  />
            </div>

            <h2>
                {"Lights - Bathroom"}
            </h2>

            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Passage/Light/split/bathroom"} action={Action::Toggle} icon={light_icon} scene={"auto"}  />
                <Button<Light2Props> name={"On"} topic_substr={"Passage/Light/split/bathroom"} action={Action::Toggle} icon={light_icon} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Passage/Light/split/bathroom"} action={Action::Toggle} icon={light_icon} scene={"rainbow"}  />
                <Button<Light2Props> name={"Busy"} topic_substr={"Passage/Light/split/bathroom"} action={Action::Toggle} icon={light_icon} scene={"busy"}  />
            </div>

            <h2>
            {"Lights - Bedroom"}
            </h2>

            <div class="buttons">
                <Button<Light2Props> name={"Auto"} topic_substr={"Passage/Light/split/bedroom"} action={Action::Toggle} icon={light_icon} scene={"auto"}  />
                <Button<Light2Props> name={"On"} topic_substr={"Passage/Light/split/bedroom"} action={Action::Toggle} icon={light_icon} scene={"on"} />
                <Button<Light2Props> name={"Rainbow"} topic_substr={"Passage/Light/split/bedroom"} action={Action::Toggle} icon={light_icon} scene={"rainbow"}  />
                <Button<Light2Props> name={"Busy"} topic_substr={"Passage/Light/split/bedroom"} action={Action::Toggle} icon={light_icon} scene={"busy"}  />
            </div>

        </RequireConnection>
    )
}

#[function_component(Tesla)]
pub fn tesla() -> Html {
    let light_icon = Icon::Light;

    html!(
        <RequireConnection>
            <h1>{ "Tesla" }</h1>

            <h2>{"Switches"}</h2>
            <div class="buttons">
                <Button<SwitchProps> name={"Charge"} topic_substr={"Tesla/1/AutoCharge"} action={Action::Toggle} icon={light_icon} />
                <Button<SwitchProps> name={"Force"} topic_substr={"Tesla/1/ForceCharge"} action={Action::Toggle} icon={light_icon} />
            </div>
        </RequireConnection>
    )
}