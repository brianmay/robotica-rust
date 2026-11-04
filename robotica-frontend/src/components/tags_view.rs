use std::sync::Arc;

use robotica_common::config::{tags_topic, Config};
use yew::prelude::*;

use crate::components::tags::RoboticaTags;

use super::require_connection::RequireConnection;

#[function_component(TagsView)]
pub fn tags_view() -> Html {
    match use_context::<Option<Arc<Config>>>() {
        Some(Some(config)) => {
            let topic = tags_topic(&config.instance);

            html! {
                <RequireConnection>
                    <h1>{ "Tags" }</h1>
                    <RoboticaTags topic={topic} />
                </RequireConnection>
            }
        }
        Some(None) => html! {
            <h1>{"Loading..."}</h1>
        },
        None => html! {
            <h1>{"Config error..."}</h1>
        },
    }
}
