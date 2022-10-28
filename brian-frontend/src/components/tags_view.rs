//! Component that shows the schedule
use itertools::sorted;
use robotica_frontend::services::protocol::Tags;
use robotica_frontend::services::websocket::Command;
use robotica_frontend::services::websocket::{protocol::MqttMessage, WebsocketService};
use yew::prelude::*;

/// Component that shows the schedule
#[function_component(TagsView)]
pub fn tags_view() -> Html {
    let wss = use_state(WebsocketService::new);

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
    let mut tx = wss.tx.clone();
    tx.try_send(subscribe)
        .unwrap_or_else(|err| log::error!("Could not send subscribe command: {err}"));

    html! {
        <>
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
            </>
    }
}
