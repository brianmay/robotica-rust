//! Component that shows the schedule
use itertools::sorted;
use tracing::error;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::services::websocket::WebsocketService;
use robotica_common::mqtt::{Json, MqttMessage};
use robotica_common::scheduler::Tags;

/// The yew properties for the schedule component
#[derive(Properties, Eq, PartialEq)]
pub struct Props {
    /// The topic to subscribe to
    pub topic: String,
}

/// Component that shows the tags
#[function_component(RoboticaTags)]
pub fn tags(props: &Props) -> Html {
    let wss: WebsocketService = use_context().unwrap();
    let subscription = use_mut_ref(|| None);
    let tags = use_state(|| None);

    let callback = {
        let tags = tags.clone();
        Callback::from(move |msg: MqttMessage| {
            msg.try_into().map_or_else(
                |e| {
                    error!("Failed to parse schedule: {}", e);
                },
                |Json(new_tags): Json<Tags>| tags.set(Some(new_tags)),
            );
        })
    };

    use_mut_ref(move || {
        let topic = props.topic.clone();
        let mut wss = wss;
        spawn_local(async move {
            let sub = wss.subscribe_mqtt(topic, callback).await;
            *subscription.borrow_mut() = Some(sub);
        });
    });

    html! {
        if let Some(tags) = &*tags {
            <div>
                {
                    for tags.iter().map(|tag| html! {
                        <div key={tag.date.to_string()}>
                            <h2>{ tag.date.format("%A, %e %B, %Y").to_string() }</h2>
                            <div class="tags">
                            {
                                sorted(tag.tags.iter()).map(|tag| {
                                    html! {
                                        <div class="tag">{tag}</div>
                                    }
                                }).collect::<Html>()
                            }
                            </div>
                        </div>
                })}
            </div>
        } else {
                <p>{ "No tags" }</p>
        }
    }
}
