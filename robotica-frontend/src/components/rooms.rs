use std::sync::Arc;

use yew::prelude::*;

use crate::components::button::{
    Button, HdmiProps, LightProps, Music2Props, SwitchProps, TasmotaProps, ZwaveProps,
};
use robotica_common::config::{ButtonConfig, Config, ControllerConfig, Icon, RoomConfig};
use robotica_common::controllers::Action;

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
        ControllerConfig::Light(config) => {
            let props = LightProps {
                name: title,
                icon,
                action: config.action,
                topic_substr: config.topic_substr.clone(),
                scene: config.scene.clone(),
            };
            html! { <Button<LightProps> ..props /> }
        }
        ControllerConfig::Music(config) => {
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
        <span key={&*button.id}>
            { controller_to_html(title, icon, &button.controller) }
        </span>
    )
}

fn rows_to_html(room: &RoomConfig) -> Html {
    html!(
        <div>
            {
            room.rows.iter().map(|row| {
                let id = format!("{}-{}", room.id, row.id);
                html!(
                    <div key={&*id}>
                        <h2>{&*row.title}</h2>
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
            <h1>{&*room.title}</h1>
            { rows_to_html(room) }
        </div>
    )
}

#[derive(Properties, Eq, PartialEq)]
pub struct Props {
    pub id: String,
}

#[function_component(Room)]
pub fn room(props: &Props) -> Html {
    let config = use_context::<Option<Arc<Config>>>();

    let rooms = match &config {
        Some(Some(config)) => &config.rooms,
        Some(None) => {
            return html! {
                <h1>{"Loading..."}</h1>
            }
        }
        None => {
            return html! {
                <h1>{"Config error..."}</h1>
            }
        }
    };

    if let Some(room) = rooms.iter().find(|room| room.id == props.id) {
        html!(
            <RequireConnection>
            { room_to_html(room) }
            </RequireConnection>
        )
    } else {
        html!(<h1>{"404 Please ask a Penguin for help"}</h1>)
    }
}
