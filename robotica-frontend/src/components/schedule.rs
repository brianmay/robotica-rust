//! Component that shows the schedule
use log::error;
use yew::prelude::*;

use robotica_common::{scheduler::Sequence, websocket::MqttMessage};
use yew_agent::Bridged;

use crate::services::websocket::event_bus::{Command, EventBus};

/// The yew properties for the websocket client
#[derive(Properties, Eq, PartialEq)]
pub struct Props {
    /// The topic to subscribe to
    pub topic: String,
}

/// Component that shows the schedule
#[function_component(Schedule)]
pub fn schedule(props: &Props) -> Html {
    let sequence_list = use_state(std::vec::Vec::new);

    let callback = {
        Callback::from(move |msg: ()| {
            error!("Received message: {:?}", msg);
        })
    };

    let events = use_mut_ref(|| EventBus::bridge(callback));

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

    use_ref(|| {
        let topic = props.topic.clone();
        let subscribe = Command::Subscribe { topic, callback };
        events.borrow_mut().send(subscribe);
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
