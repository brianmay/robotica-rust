use std::time::Duration;

use robotica_node_rust::{
    filters::teslamate::{is_insecure, requires_plugin},
    sources::mqtt::Subscriptions,
    TxPipe,
};

use super::common::{power_to_bool, string_to_bool, string_to_integer};

fn geofence_to_message((old, new): (Option<String>, String)) -> Option<String> {
    match (old.as_deref(), new.as_str()) {
        (None, _) => None,
        (Some(old), new) if old == new => None,
        (Some("nowhere"), new) => Some(format!("The tesla has arrived at {new}")),
        (Some(old), "nowhere") => Some(format!("The tesla has left {old}")),
        (Some(old), new) => Some(format!("The tesla has left {old} and arrived at {new}")),
    }
}

fn plugged_in_to_message((old, new): (Option<bool>, bool)) -> Option<String> {
    match (old, new) {
        (None, _) => None,
        (Some(false), true) => Some("The tesla has been leashed".to_string()),
        (Some(true), false) => Some("The tesla has been unleashed".to_string()),
        (Some(true), true) => None,
        (Some(false), false) => None,
    }
}

pub fn start(subscriptions: &mut Subscriptions, message_sink: &TxPipe<String>) {
    car(1, subscriptions, message_sink);
}

fn car(car_id: usize, subscriptions: &mut Subscriptions, message_sink: &TxPipe<String>) {
    let topic = format!("teslamate/cars/{car_id}/battery_level");
    let battery_level = subscriptions
        .subscribe_to_string(&topic)
        .filter_map(string_to_integer);

    let topic = format!("teslamate/cars/{car_id}/plugged_in");
    let plugged_in = subscriptions
        .subscribe_to_string(&topic)
        .filter_map(string_to_bool);

    let topic = format!("teslamate/cars/{car_id}/geofence");
    let geofence = subscriptions.subscribe_to_string(&topic);

    let topic = format!("teslamate/cars/{car_id}/is_user_present");
    let is_user_present = subscriptions
        .subscribe_to_string(&topic)
        .filter_map(string_to_bool);

    let topic = format!("teslamate/cars/{car_id}/locked");
    let locked = subscriptions
        .subscribe_to_string(&topic)
        .filter_map(string_to_bool);

    let topic = String::from("state/Brian/TeslaReminder/power");
    let reminder = subscriptions.subscribe_to_string(&topic).map(power_to_bool);

    plugged_in
        .debug("plugged_in")
        .diff()
        .filter_map(plugged_in_to_message)
        .copy_to(message_sink);

    requires_plugin(battery_level, plugged_in, geofence, reminder)
        .debug("requires_plugin")
        .diff_with_initial_value(Some(false))
        .changed()
        .timer_true(Duration::from_secs(60 * 10))
        .map(|v| {
            if v {
                "The tesla requires leashing".to_string()
            } else {
                "The tesla no longer requires leashing".to_string()
            }
        })
        .copy_to(message_sink);
}
