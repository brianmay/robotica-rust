use yew::prelude::*;

use crate::components::schedule::Schedule;

use super::require_connection::RequireConnection;

#[function_component(ScheduleView)]
pub fn schedule_view() -> Html {
    html!(
        <RequireConnection>
            <h1>{ "Schedule" }</h1>
            <Schedule topic={"schedule/robotica.linuxpenguins.xyz/pending"} />
        </RequireConnection>
    )
}
