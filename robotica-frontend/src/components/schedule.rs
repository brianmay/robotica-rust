//! Component that shows the schedule
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use robotica_common::{
    mqtt::{Json, MqttMessage},
    scheduler::Sequence,
};

use crate::services::websocket::WebsocketService;

/// The yew properties for the websocket client
#[derive(Properties, Eq, PartialEq)]
pub struct Props {
    /// The topic to subscribe to
    pub topic: String,
}

/// Component that shows the schedule
#[function_component(Schedule)]
pub fn schedule(props: &Props) -> Html {
    let wss: WebsocketService = use_context().unwrap();
    let subscription = use_mut_ref(|| None);
    let sequence_list = use_state(std::vec::Vec::new);

    let callback = {
        let sequence_list = sequence_list.clone();
        Callback::from(move |msg: MqttMessage| {
            msg.try_into().map_or_else(
                |e| {
                    tracing::error!("Failed to parse schedule: {}", e);
                },
                |Json(new_schedule): Json<Vec<Sequence>>| sequence_list.set(new_schedule),
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
        <div class="sequence_list">
        {
            sequence_list.iter().map(|sequence| {
                html! {
                    <div class="sequence" id={sequence.id.clone()}>
                        <div>{sequence.required_time.clone()}</div>
                        <div>{
                            sequence.tasks.iter().map(|task| {
                                html! {
                                    <div>{task}</div>
                                }
                            }).collect::<Html>()
                        }</div>
                    </div>
                }
            }).collect::<Html>()
        }
        </div>
    }
}
