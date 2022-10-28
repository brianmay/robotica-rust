//! Component that shows the schedule
use yew::prelude::*;

use crate::services::{
    protocol::Sequence,
    websocket::Command,
    websocket::{protocol::MqttMessage, WebsocketService},
};

/// The yew properties for the websocket client
#[derive(Properties, Eq, PartialEq)]
pub struct Props {
    /// The topic to subscribe to
    pub topic: String,
}

/// Component that shows the schedule
#[function_component(Schedule)]
pub fn schedule(props: &Props) -> Html {
    let wss = use_state(WebsocketService::new);

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

            // .map(|new_schedule: Vec<Sequence>| schedule.set(new_schedule))
            // .unwrap_or_else(|e| {
            //     log::error!("Failed to parse schedule: {}", e);
            // });
        })
    };

    let topic = props.topic.clone();
    let subscribe = Command::Subscribe { topic, callback };
    let mut tx = wss.tx.clone();
    tx.try_send(subscribe)
        .unwrap_or_else(|err| log::error!("Could not send subscribe command: {err}"));

    html! {
        <div class="sequence_list">
        {
            sequence_list.iter().map(|sequence| {
                html! {
                    <div id={sequence.id.clone()}>
                        <span>{sequence.required_time.clone()}</span>
                        <span>{format!("{}", sequence)}</span>
                    </div>
                }
            }).collect::<Html>()
        }
        </div>
    }
}
