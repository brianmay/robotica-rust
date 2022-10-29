//! Component that shows the schedule
use itertools::sorted;
use yew::prelude::*;

use robotica_common::scheduler::Tags;
use robotica_common::websocket::MqttMessage;
use robotica_frontend::services::websocket::Command;
use robotica_frontend::services::websocket::WebsocketService;

use super::require_connection::RequireConnection;

/// Component that shows the schedule
#[function_component(TagsView)]
pub fn tags_view() -> Html {
    let wss = use_context::<WebsocketService>().expect("No context found.");

    let tags = use_state(|| None);

    let callback = {
        let tags = tags.clone();
        Callback::from(move |msg: MqttMessage| {
            serde_json::from_str(&msg.payload).map_or_else(
                |e| {
                    log::error!("Failed to parse schedule: {}", e);
                },
                |new_tags: Tags| tags.set(Some(new_tags)),
            );

            // .map(|new_schedule: Vec<Sequence>| schedule.set(new_schedule))
            // .unwrap_or_else(|e| {
            //     log::error!("Failed to parse schedule: {}", e);
            // });
        })
    };

    let topic = "robotica/robotica.linuxpenguins.xyz/tags".to_string();
    let subscribe = Command::Subscribe { topic, callback };
    let mut tx = wss.tx;
    tx.try_send(subscribe)
        .unwrap_or_else(|err| log::error!("Could not send subscribe command: {err}"));

    html! {
        <RequireConnection>
            <h1>{ "Tags" }</h1>
            if let Some(tags) = &*tags {
                <div>
                    <h2>{ "Yesterday" }</h2>
                    <div class="tags">
                        {
                            sorted(tags.yesterday.iter()).map(|tag| {
                                html! {
                                    <div class="tag">{tag}</div>
                                }
                            }).collect::<Html>()
                        }

                    </div>

                    <h2>{ "Today" }</h2>
                    <div class="tags">
                        {
                            sorted(tags.today.iter()).map(|tag| {
                                html! {
                                    <div class="tag">{tag}</div>
                                }
                            }).collect::<Html>()
                        }
                    </div>

                    <h2>{ "Tomorrow" }</h2>
                    <div class="tags">
                        {
                            sorted(tags.tomorrow.iter()).map(|tag| {
                                html! {
                                    <div class="tag">{tag}</div>
                                }
                            }).collect::<Html>()
                        }
                    </div>
                </div>
            } else {
                    <p>{ "No tags" }</p>
            }
        </RequireConnection>
    }
}
