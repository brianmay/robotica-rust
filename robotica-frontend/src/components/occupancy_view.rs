use std::collections::HashMap;

use crate::{
    components::require_connection::RequireConnection,
    services::websocket::{Subscription, WebsocketService},
};
use robotica_common::{
    config::{occupancy_topic_pattern, presence_topic},
    mqtt::{Json, MqttMessage},
    robotica::occupancy::{OccupiedState, PresenceTrackerValue},
};
use tracing::debug;
use yew::prelude::*;

#[derive(Clone, Debug)]
struct PersonPresence {
    person_id: String,
    distance: Option<f32>,
}

#[allow(dead_code)]
pub enum Msg {
    SubscribedPresence(Subscription),
    SubscribedOccupancy(Subscription),
    Presence(MqttMessage),
    Occupancy(MqttMessage),
}

#[derive(Eq, PartialEq, Properties, Clone)]
pub struct Props {
    pub people: Vec<String>,
}

pub struct OccupancyViewComponent {
    presence_subscriptions: Vec<Subscription>,
    occupancy_subscription: Option<Subscription>,
    room_presences: HashMap<String, Vec<PersonPresence>>,
    person_to_room: HashMap<String, String>,
    occupancies: HashMap<String, OccupiedState>,
}

#[must_use]
fn extract_id_from_topic(topic: &str, suffix: &str) -> Option<String> {
    let prefix = "robotica/state/";
    let suffix_with_slash = format!("/{suffix}");
    if topic.starts_with(prefix) && topic.ends_with(&suffix_with_slash) {
        Some(
            topic
                .strip_prefix(prefix)?
                .strip_suffix(&suffix_with_slash)?
                .to_string(),
        )
    } else {
        None
    }
}

#[must_use]
fn extract_room_from_id(combined_id: &str) -> String {
    combined_id
        .split_once('/')
        .map_or_else(|| combined_id.to_string(), |(room, _)| room.to_string())
}

/// Split a combined occupancy id (`room/sensor`) into its `(room, sensor)` parts.
/// When the id has no `/`, the whole string is treated as the room and the sensor
/// defaults to `"default"` for backwards compatibility with the display.
#[must_use]
fn split_room_and_sensor(combined_id: &str) -> (String, String) {
    match combined_id.split_once('/') {
        Some((room, sensor)) => (room.to_string(), sensor.to_string()),
        None => (combined_id.to_string(), "default".to_string()),
    }
}

fn subscribe_presence(ctx: &Context<OccupancyViewComponent>) {
    let (wss, _): (WebsocketService, _) = ctx
        .link()
        .context(ctx.link().batch_callback(|_| None))
        .unwrap();

    for person in ctx.props().people.clone() {
        let topic = presence_topic(&person);
        let callback = ctx.link().callback(Msg::Presence);
        let mut wss = wss.clone();
        ctx.link().send_future(async move {
            let s = wss.subscribe_mqtt(topic, callback).await;
            Msg::SubscribedPresence(s)
        });
    }
}

fn subscribe_occupancy(ctx: &Context<OccupancyViewComponent>) {
    let (wss, _): (WebsocketService, _) = ctx
        .link()
        .context(ctx.link().batch_callback(|_| None))
        .unwrap();

    let topic = occupancy_topic_pattern().to_string();
    let callback = ctx.link().callback(Msg::Occupancy);
    let mut wss = wss;
    ctx.link().send_future(async move {
        let s = wss.subscribe_mqtt(topic, callback).await;
        Msg::SubscribedOccupancy(s)
    });
}

impl Component for OccupancyViewComponent {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        subscribe_presence(ctx);
        subscribe_occupancy(ctx);
        Self {
            presence_subscriptions: Vec::new(),
            occupancy_subscription: None,
            room_presences: HashMap::new(),
            person_to_room: HashMap::new(),
            occupancies: HashMap::new(),
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {}
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SubscribedPresence(subscription) => {
                self.presence_subscriptions.push(subscription);
                false
            }
            Msg::SubscribedOccupancy(subscription) => {
                self.occupancy_subscription = Some(subscription);
                false
            }
            Msg::Presence(msg) => {
                debug!("Presence message: {:?}", msg);
                let mut changed = false;
                if let Some(person_id) = extract_id_from_topic(&msg.topic, "presence") {
                    if let Ok(Json(value)) = Json::<PresenceTrackerValue>::try_from(msg) {
                        let old_room = self.person_to_room.get(&person_id).cloned();
                        let new_room = value.room.clone();

                        if old_room != new_room {
                            if let Some(old) = old_room {
                                if let Some(list) = self.room_presences.get_mut(&old) {
                                    list.retain(|p| p.person_id != person_id);
                                }
                            }
                            if let Some(new) = &new_room {
                                let entry = self.room_presences.entry(new.clone()).or_default();
                                if !entry.iter().any(|p| p.person_id == person_id) {
                                    entry.push(PersonPresence {
                                        person_id: person_id.clone(),
                                        distance: value.distance,
                                    });
                                }
                            }
                            match new_room {
                                Some(new_room) => {
                                    self.person_to_room.insert(person_id, new_room);
                                }
                                None => {
                                    self.person_to_room.remove(&person_id);
                                }
                            }
                            changed = true;
                        }
                    }
                }
                changed
            }
            Msg::Occupancy(msg) => {
                debug!("Occupancy message: {:?}", msg);
                let mut changed = false;
                if let Some(id) = extract_id_from_topic(&msg.topic, "occupancy") {
                    if let Ok(Json(value)) = msg.try_into() {
                        self.occupancies.insert(id, value);
                        changed = true;
                    }
                }
                changed
            }
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        let mut all_rooms: Vec<String> = self.room_presences.keys().cloned().collect();
        all_rooms.extend(self.occupancies.keys().map(|id| extract_room_from_id(id)));
        all_rooms.sort();
        all_rooms.dedup();

        // Group occupancy sensors by room: `room -> Vec<(sensor_id, OccupiedState)>`,
        // supporting multiple occupancy sensors per room rather than a single
        // hardcoded "default" sensor.
        let mut room_sensors: HashMap<String, Vec<(String, OccupiedState)>> = HashMap::new();
        for (id, state) in &self.occupancies {
            let (room, sensor) = split_room_and_sensor(id);
            room_sensors.entry(room).or_default().push((sensor, *state));
        }
        for sensors in room_sensors.values_mut() {
            sensors.sort_by(|a, b| a.0.cmp(&b.0));
        }

        html! {
            <RequireConnection>
                <div class="container">
                    <h1>{ "Occupancy" }</h1>
                    if all_rooms.is_empty() {
                        <p>{"No occupancy or presence data received yet."}</p>
                    } else {
                        <table class="table table-striped">
                            <thead>
                                <tr>
                                    <th scope="col">{"Room"}</th>
                                    <th scope="col">{"Presence"}</th>
                                    <th scope="col">{"Occupancy"}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {
                                    all_rooms.iter().map(|room| {
                                        let presences = self.room_presences.get(room);
                                        let sensors = room_sensors.get(room);
                                        html! {
                                            <tr>
                                                <td>{ room }</td>
                                                <td>
                                                    {
                                                        if let Some(people) = presences {
                                                            if people.is_empty() {
                                                                html! { "—" }
                                                            } else {
                                                                html! {
                                                                    { people.iter().map(|p| {
                                                                        let dist = p.distance.map(|d| format!(" ({d:.1}m)")).unwrap_or_default();
                                                                        html! {
                                                                            <span>{ format!("{}{}", p.person_id, dist) }</span>
                                                                        }
                                                                    }).collect::<Html>() }
                                                                }
                                                            }
                                                        } else {
                                                            html! { "—" }
                                                        }
                                                    }
                                                </td>
                                                <td>
                                                    {
                                                        if let Some(sensors) = sensors {
                                                            if sensors.is_empty() {
                                                                html! { "—" }
                                                            } else {
                                                                html! {
                                                                    { sensors.iter().map(|(sensor, state)| {
                                                                        let label = if sensor == "default" {
                                                                            String::new()
                                                                        } else {
                                                                            format!("{sensor}: ")
                                                                        };
                                                                        let state_text = if state.is_occupied() { "Occupied" } else { "Vacant" };
                                                                        html! {
                                                                            <div>{ format!("{label}{state_text}") }</div>
                                                                        }
                                                                    }).collect::<Html>() }
                                                                }
                                                            }
                                                        } else {
                                                            html! { "—" }
                                                        }
                                                    }
                                                </td>
                                            </tr>
                                        }
                                    }).collect::<Html>()
                                }
                            </tbody>
                        </table>
                    }
                </div>
            </RequireConnection>
        }
    }
}
