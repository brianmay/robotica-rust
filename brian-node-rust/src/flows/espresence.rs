use std::time::Duration;

use robotica_node_rust::sources::mqtt::Subscriptions;
use serde::Deserialize;

use log::*;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct Beacon {
    id: String,
    id_type: u32,
    rssi: i32,
    raw: f32,
    distance: f32,
    speed: f32,
    mac: String,
    interval: Option<u32>,
}

pub fn brian_in_room(
    room: &str,
    subscriptions: &mut Subscriptions,
) -> robotica_node_rust::RxPipe<bool> {
    let topic =
        &format!("espresense/devices/iBeacon:63a1368d-552b-4ea3-aed5-b5fefb2adf09-99-86/{room}");
    subscriptions
        .subscribe_to_string(topic)
        .filter_map(|s| {
            let b: Option<Beacon> = serde_json::from_str(&s)
                .map_err(|err| {
                    error!("Got invalid espresense message: {}", err);
                    err
                })
                .ok();
            b
        })
        .map(|b| b.distance < 20.0)
        .debug(&format!("Brian is in {room}"))
        .startup_delay(Duration::from_secs(10), false)
        .delay_cancel(Duration::from_secs(15))
        .debug(&format!("Brian is in {room} (delayed)"))
}
