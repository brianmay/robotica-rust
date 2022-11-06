//! Component that shows the most recent message from a topic
#![allow(clippy::option_if_let_else)]
use std::fmt::Display;

use yew::prelude::*;

use robotica_common::mqtt::MqttMessage;
use yew_agent::Bridged;

use crate::services::websocket::event_bus::{Command, EventBus};

/// The yew properties for the websocket client
#[derive(Properties, Eq, PartialEq)]
pub struct Props {
    /// The topic to subscribe to
    pub topic: String,
}

/// Component that shows the schedule
#[function_component(MqttLast)]
pub fn mqtt_last<T>(props: &Props) -> Html
where
    T: TryFrom<MqttMessage> + Display + 'static,
    T::Error: std::fmt::Debug + std::fmt::Display,
{
    let message = use_state::<Option<T>, _>(|| None);

    let callback = { Callback::from(move |_| {}) };

    let events = use_mut_ref(|| EventBus::bridge(callback));

    let callback = {
        let message = message.clone();
        Callback::from(move |msg: MqttMessage| {
            T::try_from(msg).map_or_else(
                |e| {
                    log::error!("Failed to parse message: {}", e);
                },
                |new_message| message.set(Some(new_message)),
            );
        })
    };

    use_ref(|| {
        let topic = props.topic.clone();
        let subscribe = Command::Subscribe { topic, callback };
        events.borrow_mut().send(subscribe);
    });

    html! {
        if let Some(message) = &*message {
            { message }
        } else {
            { "unknown" }
        }
    }
}
