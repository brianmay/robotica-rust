use yew::prelude::*;

use robotica_frontend::components::schedule::Schedule;
use robotica_frontend::components::ws_client::WsClient;

use super::require_connection::RequireConnection;

#[function_component(ScheduleView)]
pub fn schedule_view() -> Html {
    html!(
        <WsClient>
            <RequireConnection>
                <h1>{ "Schedule" }</h1>
                <Schedule topic={"schedule/robotica.linuxpenguins.xyz"} />
            </RequireConnection>
        </WsClient>
    )
}
