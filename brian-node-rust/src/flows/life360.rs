use std::collections::HashMap;

use paho_mqtt::Message;
use robotica_node_rust::{
    filters::{ChainGeneric, ChainSplit},
    sources::{
        life360::{self, Member},
        mqtt::{MqttMessage, Subscriptions},
        ChainMqtt,
    },
};
use tokio::sync::mpsc::Sender;

use super::common::ChainMessage;

type MemberIndex = HashMap<String, Member>;

#[derive(Clone)]
struct Changed {
    old_location: Option<String>,
    new_location: Option<String>,
    changed: bool,
    member: Member,
}

fn member_diff(index: &mut MemberIndex, member: Member) -> (Option<Member>, Member) {
    let id = member.id.clone();

    let rc = match index.get(&id) {
        None => (None, member.clone()),
        old_value => (old_value.cloned(), member.clone()),
    };

    index.insert(id, member);
    rc
}

fn member_location_changed((old, new): (Option<Member>, Member)) -> Changed {
    match (old, new) {
        (None, new) => Changed {
            changed: false,
            old_location: None,
            new_location: new.location.name.clone(),
            member: new,
        },
        (Some(old), new) => Changed {
            changed: old.location.name != new.location.name,
            old_location: old.location.name,
            new_location: new.location.name.clone(),
            member: new,
        },
    }
}

fn member_changed((old, new): (Option<Member>, Member)) -> Option<Member> {
    match (old, new) {
        (None, new) => Some(new),
        (Some(old), new) => {
            if old.same_values(&new) {
                None
            } else {
                Some(new)
            }
        }
    }
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
    let circles = life360::circles().map_with_state(HashMap::new(), member_diff);
    let (circles1, circles2) = circles.split2();

    circles1
        .filter_map(member_changed)
        .map(|m| {
            let topic = format!("life360/{}", m.id);
            let payload = serde_json::to_string(&m).unwrap();
            Message::new_retained(topic, payload, 0)
        })
        .publish(mqtt_out.clone());

    circles2
        .map(member_location_changed)
        .filter_map(changed_to_message)
        .message(subscriptions, mqtt_out);
}
