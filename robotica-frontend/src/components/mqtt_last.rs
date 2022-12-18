//! Component that shows the most recent message from a topic
#![allow(clippy::option_if_let_else)]
use std::fmt::Display;

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use robotica_common::mqtt::MqttMessage;

use crate::services::websocket::WebsocketService;

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
    let wss: WebsocketService = use_context().unwrap();
    let subscription = use_mut_ref(|| None);
    let message = use_state::<Option<T>, _>(|| None);

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

    use_ref(move || {
        let topic = props.topic.clone();
        let mut wss = wss;
        spawn_local(async move {
            let sub = wss.subscribe_mqtt(topic, callback).await;
            *subscription.borrow_mut() = Some(sub);
        });
    });

    html! {
        if let Some(message) = &*message {
            { message }
        } else {
            { "unknown" }
        }
    }
}
