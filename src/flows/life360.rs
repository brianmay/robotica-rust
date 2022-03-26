use std::collections::HashMap;

use paho_mqtt::Message;
use robotica_node_rust::{
    filters::{ChainDebug, ChainGeneric, ChainSplit},
    sources::{
        life360::{self, Member},
        mqtt::{MqttMessage, Subscriptions},
        ChainMqtt,
    },
};
use tokio::sync::mpsc::Sender;

use super::common::ChainMessage;

type Index = HashMap<String, Option<String>>;

#[derive(Clone)]
struct Changed {
    old_location: Option<String>,
    new_location: Option<String>,
    changed: bool,
    member: Member,
}

fn life360_location_changed(index: &mut Index, member: Member) -> Changed {
    let id = member.id.clone();
    let new_location = member.location.name.clone();

    let rc = match index.get(&id) {
        Some(old_location) => Changed {
            changed: *old_location != new_location,
            old_location: old_location.clone(),
            new_location: new_location.clone(),
            member,
        },
        None => Changed {
            changed: false,
            old_location: None,
            new_location: new_location.clone(),
            member,
        },
    };

    index.insert(id, new_location);
    rc
}

fn changed_to_message(changed: Changed) -> Option<String> {
    let name = format!("{} {}", changed.member.first_name, changed.member.last_name);

    match changed {
        Changed { changed: false, .. } => None,

        Changed {
            old_location: None,
            new_location: Some(new_location),
            ..
        } => Some(format!("{name} has arrived at {new_location}")),

        Changed {
            old_location: Some(old_location),
            new_location: None,
            ..
        } => Some(format!("{name} has left at {old_location}")),

        Changed {
            old_location: Some(old_location),
            new_location: Some(new_location),
            ..
        } => Some(format!(
            "{name} has left {old_location} and arrived at {new_location}"
        )),

        Changed { .. } => None,
    }
}

pub fn start(subscriptions: &mut Subscriptions, mqtt_out: &Sender<MqttMessage>) {
    let circles = life360::circles().debug("circles".to_string());

    let (circles1, circles2) = circles.split2();

    circles1
        .map(|m| {
            let topic = format!("life360/{}", m.id);
            let payload = serde_json::to_string(&m).unwrap();
            Message::new(topic, payload, 0)
        })
        .publish(mqtt_out.clone());

    circles2
        .map_with_state(HashMap::new(), life360_location_changed)
        .filter_map(changed_to_message)
        .message(subscriptions, mqtt_out);
}
