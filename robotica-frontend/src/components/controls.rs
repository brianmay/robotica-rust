use yew::prelude::*;

use crate::components::button::{Button, ButtonProps};
use robotica_common::config::{ButtonConfig, ButtonRowConfig, ControllerConfig, Icon};
use robotica_common::controllers::robotica::{hdmi_matrix, lights, music, switch};
use robotica_common::controllers::{tasmota, zwave, Action};

use super::require_connection::RequireConnection;

fn controller_to_html(title: String, icon: Icon, controller_config: &ControllerConfig) -> Html {
    match controller_config {
        ControllerConfig::HdmiMatrix(config) => {
            let props = ButtonProps {
                title,
                icon,
                config: config.clone(),
            };
            html! { <Button<hdmi_matrix::Config> ..props /> }
        }
        ControllerConfig::Light(config) => {
            let props = ButtonProps {
                title,
                icon,
                config: config.clone(),
            };
            html! { <Button<lights::Config> ..props /> }
        }
        ControllerConfig::Music(config) => {
            let props = ButtonProps {
                title,
                icon,
                config: config.clone(),
            };
            html! { <Button<music::Config> ..props /> }
        }
        ControllerConfig::Switch(config) => {
            let props = ButtonProps {
                title,
                icon,
                config: config.clone(),
            };
            html! { <Button<switch::Config> ..props /> }
        }
        ControllerConfig::Zwave(config) => {
            let props = ButtonProps {
                title,
                icon,
                config: config.clone(),
            };
            html! { <Button<zwave::Config> ..props /> }
        }
        ControllerConfig::Tasmota(config) => {
            let props = ButtonProps {
                title,
                icon,
                config: tasmota::Config {
                    topic_substr: config.topic_substr.clone(),
                    action: Action::Toggle,
                    power_postfix: config.power_postfix.clone(),
                },
            };
            html! { <Button<tasmota::Config> ..props /> }
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

fn rows_to_html(view_id: &str, rows: &[ButtonRowConfig]) -> Html {
    html!(
        <div>
            {
            rows.iter().map(|row| {
                let id = format!("{}-{}", view_id, row.id);
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

/// Properties for a controls view.
#[derive(Properties, Eq, PartialEq, Clone)]
pub struct Props {
    /// The id of the view, used to key the button rows.
    pub id: String,

    /// The title of the view.
    pub title: String,

    /// The rows of buttons to display.
    pub rows: Vec<ButtonRowConfig>,
}

/// A controls view: a grid of button rows.
#[function_component(Controls)]
pub fn controls(props: &Props) -> Html {
    html!(
        <RequireConnection>
            <div>
                <h1>{&*props.title}</h1>
                { rows_to_html(&props.id, &props.rows) }
            </div>
        </RequireConnection>
    )
}
