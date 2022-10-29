//! Component that shows the schedule
use yew::prelude::*;

use robotica_common::{scheduler::Sequence, websocket::MqttMessage};

use crate::services::{websocket::Command, websocket::WebsocketService};

/// The yew properties for the websocket client
#[derive(Properties, Eq, PartialEq)]
pub struct Props {
    /// The topic to subscribe to
    pub topic: String,
}

/// Component that shows the schedule
#[function_component(Schedule)]
pub fn schedule(props: &Props) -> Html {
    let wss = use_context::<WebsocketService>().expect("No context found.");

    let sequence_list = use_state(std::vec::Vec::new);

    let callback = {
        let sequence_list = sequence_list.clone();
        Callback::from(move |msg: MqttMessage| {
            serde_json::from_str(&msg.payload).map_or_else(
                |e| {
                    log::error!("Failed to parse schedule: {}", e);
                },
                |new_schedule: Vec<Sequence>| sequence_list.set(new_schedule),
            );
        })
    };

    let topic = props.topic.clone();
    let subscribe = Command::Subscribe { topic, callback };
    let mut tx = wss.tx;
    tx.try_send(subscribe)
        .unwrap_or_else(|err| log::error!("Could not send subscribe command: {err}"));

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
