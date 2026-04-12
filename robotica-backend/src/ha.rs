use std::collections::HashMap;
use std::sync::Arc;

use robotica_common::mqtt::Json;
use robotica_common::mqtt::QoS;
use robotica_common::mqtt::Retain;
use robotica_common::robotica::commands::Command;
// ...existing code...
use robotica_common::robotica::message::Message;
use robotica_tokio::devices::presence_tracker;
use robotica_tokio::devices::presence_tracker::get_room_for_id;
use robotica_tokio::pipes::stateful;
use robotica_tokio::pipes::stateless;
use robotica_tokio::services::mqtt::MqttTx;
use tracing::debug;
use tracing::info;
// ...existing code...

use crate::config::MessageRouteConfig;

pub fn create_message_sink<S: 'static + ::std::hash::BuildHasher + Send>(
    mqtt: MqttTx,
    message_routes: Vec<MessageRouteConfig>,
    presence_trackers: &HashMap<
        String,
        stateful::Receiver<presence_tracker::PresenceTrackerValue>,
        S,
    >,
) -> stateless::Sender<Message> {
    let presence_trackers_for_routes = Arc::new(
        message_routes
            .iter()
            .map(|route| {
                route
                    .presence_requirements
                    .iter()
                    .map(|req| get_room_for_id(&req.presence_id, presence_trackers))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
    );
    let message_routes = Arc::new(message_routes);

    let (tx, rx) = stateless::create_pipe::<Message>("messages");

    rx.for_each_async({
        let message_routes = Arc::clone(&message_routes);
        let presence_trackers_for_routes = Arc::clone(&presence_trackers_for_routes);
        move |message| {
            info!(
                title = message.title,
                message_body = message.body,
                priority = ?message.priority,
                audience = ?message.audience,
                flash_lights = message.flash_lights,
                "Received message to route"
            );
            let message_routes = Arc::clone(&message_routes);
            let presence_trackers_for_routes = Arc::clone(&presence_trackers_for_routes);
            let mqtt = mqtt.clone();
            async move {
                for (route, presence_trackers) in message_routes
                    .iter()
                    .zip(presence_trackers_for_routes.iter())
                {
                    let mut presence_matches = presence_trackers.is_empty();

                    for (req, presence_tracker) in route
                        .presence_requirements
                        .iter()
                        .zip(presence_trackers.iter())
                    {
                        if let Some(room) = presence_tracker.get().await {
                            if room.as_ref() == Some(&req.room) {
                                presence_matches = true;
                            }
                        }
                    }

                    let audience_matches = route.audience.contains(&message.audience.to_string())
                        || route.audience.is_empty();

                    debug!(
                        topic = route.topic.as_str(),
                        matches_presence = presence_matches,
                        audience_matches = audience_matches,
                        "Checking message route presence requirements"
                    );
                    if presence_matches && audience_matches {
                        debug!(
                            title = message.title,
                            message_body = message.body,
                            priority = ?message.priority,
                            audience = ?message.audience,
                            flash_lights = message.flash_lights,
                            router_topic = route.topic,
                            "Routing message"
                        );
                        let command = Command::Message(message.clone());
                        mqtt.try_serialize_send(
                            &route.topic,
                            &Json(command),
                            Retain::NoRetain,
                            QoS::ExactlyOnce,
                        );
                    }
                }
            }
        }
    });

    tx
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use robotica_common::robotica::{audio::MessagePriority, message::Audience};
    use serde_json::json;

    #[test]
    fn test_message_command() {
        let message = Message {
            title: "Title".to_string(),
            body: "Body".to_string(),
            priority: MessagePriority::Low,
            audience: Audience::new("everyone"),
            flash_lights: false,
        };
        let json = json!({
            "title": "Title",
            "body": "Body",
            "priority": "Low",
            "audience": "everyone",
            "flash_lights": false,
        });
        assert_eq!(json, serde_json::to_value(message).unwrap());
    }
}
