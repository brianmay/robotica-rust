use std::time::Duration;

use robotica_node_rust::{
    filters::{
        teslamate::{is_insecure, requires_plugin},
        ChainChanged, ChainDebug, ChainGeneric, ChainSplit, ChainTimer,
    },
    sources::mqtt::{MqttMessage, Subscriptions},
};
use tokio::sync::mpsc::Sender;

use super::common::{power_to_bool, string_to_bool, string_to_integer, ChainMessage};

fn geofence_to_message((old, new): (String, String)) -> String {
    match (old.as_str(), new.as_str()) {
        ("", new) => format!("The tesla has arrived at {new}"),
        (old, "") => format!("The tesla has left {old}"),
        (old, new) => format!("The tesla has left {old} and arrived at {new}"),
    }
}

fn plugged_in_to_message((old, new): (bool, bool)) -> String {
    match (old, new) {
        (false, true) => "The tesla has been plugged in".to_string(),
        (true, false) => "The tesla been disconnected".to_string(),
        (true, true) => "The tesla is still plugged in".to_string(),
        (false, false) => "The tesla is still disconnected".to_string(),
    }
}

pub fn start(subscriptions: &mut Subscriptions, mqtt_out: &Sender<MqttMessage>) {
    car(1, subscriptions, mqtt_out);
}

fn car(car_id: usize, subscriptions: &mut Subscriptions, mqtt_out: &Sender<MqttMessage>) {
    let topic = format!("teslamate/cars/{car_id}/battery_level");
    let battery_level = subscriptions
        .subscribe(&topic)
        .filter_map(string_to_integer);

    let topic = format!("teslamate/cars/{car_id}/plugged_in");
    let plugged_in = subscriptions.subscribe(&topic).filter_map(string_to_bool);

    let topic = format!("teslamate/cars/{car_id}/geofence");
    let geofence = subscriptions.subscribe(&topic);

    let topic = format!("teslamate/cars/{car_id}/is_user_present");
    let is_user_present = subscriptions.subscribe(&topic).filter_map(string_to_bool);

    let topic = format!("teslamate/cars/{car_id}/locked");
    let locked = subscriptions.subscribe(&topic).filter_map(string_to_bool);

    let topic = String::from("state/Brian/TeslaReminder/power");
    let reminder = subscriptions.subscribe(&topic).map(power_to_bool);

    let (plugged_in1, plugged_in2) = plugged_in.split2();
    let (geofence1, geofence2) = geofence.split2();

    geofence1
        .debug("geofence".to_string())
        .has_changed()
        .map(geofence_to_message)
        .message(subscriptions, mqtt_out);

    plugged_in1
        .debug("plugged_in".to_string())
        .has_changed()
        .map(plugged_in_to_message)
        .message(subscriptions, mqtt_out);

    is_insecure(is_user_present, locked)
        .has_changed()
        .map(|v| v.1)
        .delay_true(Duration::from_secs(60 * 2))
        .timer(Duration::from_secs(60 * 10))
        .map(|v| {
            if v {
                "The tesla is lonely and insecure".to_string()
            } else {
                "The tesla is secure and has many friends".to_string()
            }
        })
        .message(subscriptions, mqtt_out);

    requires_plugin(battery_level, plugged_in2, geofence2, reminder)
        .has_changed()
        .map(|v| v.1)
        .timer(Duration::from_secs(60 * 10))
        .map(|v| {
            if v {
                "The tesla requires plugging in".to_string()
            } else {
                "The tesla no longer requires plugging in".to_string()
            }
        })
        .message(subscriptions, mqtt_out);
}
