use std::sync::Arc;

use robotica_common::config::Config;
use yew::prelude::*;

use crate::components::schedule::RoboticaSchedule;

use super::require_connection::RequireConnection;

#[function_component(ScheduleView)]
pub fn schedule_view() -> Html {
    match use_context::<Option<Arc<Config>>>() {
        Some(Some(config)) => {
            let topic = format!("schedule/{}/pending", config.instance);

            html! {
                <RequireConnection>
                    <h1>{ "Schedule" }</h1>
                    <RoboticaSchedule topic={topic} />
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
