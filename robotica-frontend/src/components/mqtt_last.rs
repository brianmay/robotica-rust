//! Component that shows the most recent message from a topic
#![allow(clippy::option_if_let_else)]
use std::fmt::Display;

use yew::prelude::*;

use robotica_common::websocket::MqttMessage;
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
    T: From<String> + Display + 'static,
{
    let message = use_state::<Option<T>, _>(|| None);

    let callback = { Callback::from(move |_| {}) };

    let events = use_mut_ref(|| EventBus::bridge(callback));

    let callback = {
        let message = message.clone();
        Callback::from(move |msg: MqttMessage| {
            message.set(Some(T::from(msg.payload)));
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
