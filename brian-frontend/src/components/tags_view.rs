//! Component that shows the schedule
use itertools::sorted;
use tracing::error;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use robotica_common::mqtt::MqttMessage;
use robotica_common::scheduler::Tags;
use robotica_frontend::services::websocket::WebsocketService;

use super::require_connection::RequireConnection;

/// Component that shows the schedule
#[function_component(TagsView)]
pub fn tags_view() -> Html {
    let wss: WebsocketService = use_context().unwrap();
    let subscription = use_mut_ref(|| None);
    let tags = use_state(|| None);

    let callback = {
        let tags = tags.clone();
        Callback::from(move |msg: MqttMessage| {
            serde_json::from_str(&msg.payload).map_or_else(
                |e| {
                    error!("Failed to parse schedule: {}", e);
                },
                |new_tags: Tags| tags.set(Some(new_tags)),
            );
        })
    };

    use_mut_ref(move || {
        let topic = "robotica/robotica.linuxpenguins.xyz/tags".to_string();
        let mut wss = wss;
        spawn_local(async move {
            let sub = wss.subscribe_mqtt(topic, callback).await;
            *subscription.borrow_mut() = Some(sub);
        });
    });

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
