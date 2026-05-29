use std::{collections::HashMap, sync::Arc};

use gloo_timers::callback::Interval;

use crate::{
    components::locations::{editor::EditorView, list::List},
    robotica_wasm::{
        draw_control,
        robotica::{Button, ButtonOptions},
    },
    services::websocket::{Subscription, WebsocketService, WsEvent},
};
use chrono::Utc;
use geo::coord;
use gloo_utils::document;
use js_sys::Reflect;
use leaflet::{LatLng, Map, MapOptions, TileLayer};
use robotica_common::{
    mqtt::{Json, MqttMessage},
    robotica::zones::{CreateZone, LocationMessage, Zone},
};
use tap::{Pipe, Tap};
use tracing::debug;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{Element, HtmlElement, Node};
use yew::prelude::*;

use super::{
    editor::UpdateZone,
    zones::{LoadingStatus, ZoneStatus},
    ActionZone,
};

pub enum Msg {
    TrackedObject(String, LocationMessage),
    SubscribedTracked(Subscription),
    SubscribedEvents(Subscription),
    MqttEvent(WsEvent),
    CreatePolygon(leaflet::Polygon),
    UpdatePolygon(leaflet::Polygon),
    UpdateZone(UpdateZone),
    SelectZone(Zone),
    ShowList,
    SaveZone,
    DeleteItemZone,
    CancelZone,
    CancelList,
    Tick,
}

#[derive(Debug, PartialEq, Eq)]
enum Connected {
    Connected,
    Disconnected { reason: String },
}

#[derive(PartialEq, Clone)]
pub enum ParamObject {
    List(Arc<Vec<Zone>>),
    Item(ActionZone),
}

struct MapZone {
    zone: Zone,
    leaflet_id: i32,
}

struct MapActionZone {
    zone: ActionZone,
    leaflet_id: i32,
}

enum MapObject {
    List(Arc<Vec<Zone>>, Vec<MapZone>, bool),
    Item(MapActionZone),
    None,
}

impl MapObject {
    // fn get_location_by_id(&self, id: i32) -> Option<&Location> {
    //     match self {
    //         MapObject::List(locations) => locations
    //             .iter()
    //             .find(|location| location.leaflet_id == id)
    //             .map(|location| &location.location),
    //         MapObject::Item(_location) => None,
    //         MapObject::None => None,
    //     }
    // }

    fn get_action_zone_by_id(&self, id: i32) -> Option<ActionZone> {
        match self {
            MapObject::List(_, zones, _) => zones
                .iter()
                .find(|zone| zone.leaflet_id == id)
                .map(|zone| ActionZone::Update(zone.zone.clone())),

            MapObject::Item(zone) => {
                if zone.leaflet_id == id {
                    Some(zone.zone.clone())
                } else {
                    None
                }
            }
            MapObject::None => None,
        }
    }
}

enum SubscriptionStatus {
    InProgress,
    #[allow(dead_code)]
    Subscribed(Subscription),
    Unsubscribed,
}

pub struct MapComponent {
    map: Map,
    object: MapObject,
    container: HtmlElement,
    draw_layer: leaflet::FeatureGroup,
    draw_control: draw_control::DrawControl,
    _create_handler: Closure<dyn FnMut(leaflet::Event)>,
    _update_handler: Closure<dyn FnMut(leaflet::Event)>,
    _show_locations_handler: Closure<dyn FnMut(leaflet::Event)>,
    tracked_subscription: SubscriptionStatus,
    event_subscription: Option<Subscription>,
    tracked_objects: HashMap<String, (LocationMessage, leaflet::Marker)>,
    connected: Connected,
    _tick_interval: Interval,
}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub object: ParamObject,
    pub create_zone: Callback<CreateZone>,
    pub update_zone: Callback<ActionZone>,
    pub delete_zone: Callback<ActionZone>,
    pub save_zone: Callback<ActionZone>,
    pub request_item: Callback<Zone>,
    pub request_list: Callback<()>,
    pub status: ZoneStatus,
    pub loading_status: LoadingStatus,
}

impl MapComponent {
    fn render_map(&self) -> Html {
        let node: &Node = &self.container.clone().into();
        Html::VRef(node.clone())
    }

    fn set_item(&mut self, zone: ActionZone) {
        self.draw_layer.clear_layers();
        let marked_zones = self.get_marked_zones();
        let options = get_action_zone_options(&marked_zones, &zone);

        self.draw_layer.clear_layers();
        let lat_lngs = zone
            .bounds()
            .exterior()
            .coords()
            .map(|lat_lng| LatLng::new(lat_lng.y, lat_lng.x))
            .map(JsValue::from)
            .collect();

        let id = leaflet::Polygon::new_with_options(&lat_lngs, &options)
            .unchecked_into::<leaflet::Layer>()
            .add_to_layer_group(&self.draw_layer)
            .pipe(|x| self.draw_layer.get_layer_id(&x));

        self.object = MapObject::Item(MapActionZone {
            zone,
            leaflet_id: id,
        });
    }

    fn set_list(&mut self, zones: &Arc<Vec<Zone>>) {
        self.draw_layer.clear_layers();
        let marked_zones = self.get_marked_zones();

        let list: Vec<MapZone> = zones
            .iter()
            .map(|zone| {
                let options = get_zone_options(&marked_zones, zone);

                let lat_lngs = zone
                    .bounds
                    .exterior()
                    .coords()
                    .map(|lat_lng| LatLng::new(lat_lng.y, lat_lng.x))
                    .map(JsValue::from)
                    .collect();

                let polygon = leaflet::Polygon::new_with_options(&lat_lngs, &options)
                    .unchecked_into::<leaflet::Layer>()
                    .add_to_layer_group(&self.draw_layer);

                let id = self.draw_layer.get_layer_id(&polygon);

                MapZone {
                    zone: zone.clone(),
                    leaflet_id: id,
                }
            })
            .collect();

        self.object = MapObject::List(zones.clone(), list, false);
    }

    fn get_marked_zones(&self) -> Vec<i32> {
        self.tracked_objects
            .values()
            .flat_map(|(loc, _)| loc.zones.iter().map(|z| z.id))
            .collect()
    }

    fn set_object(&mut self, object: &ParamObject) {
        match object {
            ParamObject::List(zones) => self.set_list(zones),
            ParamObject::Item(zone) => self.set_item(zone.clone()),
        }
    }

    fn iterate_over_layers(&self, f: impl Fn(&ActionZone, leaflet::Layer)) {
        match &self.object {
            MapObject::List(_, zones, _) => {
                for zone in zones {
                    let layer = self.draw_layer.get_layer(zone.leaflet_id);
                    let zone = ActionZone::Update(zone.zone.clone());
                    f(&zone, layer);
                }
            }
            MapObject::Item(zone) => {
                let id = zone.leaflet_id;
                let layer = self.draw_layer.get_layer(id);
                f(&zone.zone, layer);
            }
            MapObject::None => {}
        }
    }

    fn update_zone_styles(&self) {
        let marked_zones = self.get_marked_zones();

        self.iterate_over_layers(|zone, layer| {
            // let layer: leaflet::Polygon = layer.dyn_into().unwrap();
            let layer = layer.unchecked_into::<leaflet::Polyline>();
            let options = get_action_zone_options(&marked_zones, zone);
            layer.set_style(&options);
        });
    }

    #[allow(clippy::cognitive_complexity)]
    fn position_map(&self) {
        match &self.object {
            MapObject::None => {
                self.map.fit_world();
            }
            MapObject::List(_, zones, _) => {
                if zones.is_empty() {
                    self.map.fit_world();
                } else {
                    self.map.fit_bounds(self.draw_layer.get_bounds().as_ref());
                }
            }
            MapObject::Item(_zone) => {
                self.map.fit_bounds(self.draw_layer.get_bounds().as_ref());
            }
        }
    }
}

fn get_action_zone_options(marked_zones: &[i32], zone: &ActionZone) -> leaflet::PolylineOptions {
    let color = get_action_zone_color(marked_zones, zone);
    let options = leaflet::PolylineOptions::default();
    options.set_color(color.clone());
    options.set_fill_color(color);
    options.set_weight(3.0);
    options.set_opacity(0.5);
    options.set_fill(true);
    options
}

fn get_zone_options(marked_zones: &[i32], zone: &Zone) -> leaflet::PolylineOptions {
    let color = get_zone_color(marked_zones, zone);
    let options = leaflet::PolylineOptions::default();
    options.set_color(color.clone());
    options.set_fill_color(color);
    options.set_weight(3.0);
    options.set_opacity(0.5);
    options.set_fill(true);
    options
}

fn get_action_zone_color(marked_zones: &[i32], zone: &ActionZone) -> String {
    match zone {
        ActionZone::Create(_zone) => "black".to_string(),
        ActionZone::Update(zone) => get_zone_color(marked_zones, zone),
    }
}

fn get_zone_color(marked_zones: &[i32], zone: &Zone) -> String {
    let is_marked = marked_zones.contains(&zone.id);

    if is_marked {
        "red".to_string()
    } else {
        zone.color.clone()
    }
}

impl Component for MapComponent {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        {
            let (wss, _): (WebsocketService, _) = ctx
                .link()
                .context(ctx.link().batch_callback(|_| None))
                .unwrap();

            {
                let mut wss = wss;
                let callback = ctx.link().callback(Msg::MqttEvent);

                ctx.link().send_future(async move {
                    let s = wss.subscribe_events(callback).await;
                    Msg::SubscribedEvents(s)
                });
            }
        }

        let object = &ctx.props().object;

        let container: Element = document().create_element("div").unwrap();
        let container: HtmlElement = container.dyn_into().unwrap();
        container.set_class_name("map");
        let leaflet_map = Map::new_with_element(&container, &MapOptions::default()).unwrap();

        let draw_layer = leaflet::FeatureGroup::new();
        draw_layer.add_to(&leaflet_map);

        let draw_control = draw_control(&draw_layer);
        if !matches!(object, ParamObject::Item(_)) {
            leaflet_map.add_control(&draw_control);
        }

        let create_handler = create_handler(ctx);
        let update_handler = update_handler(ctx);
        let show_list_handler = {
            let callback = ctx.link().callback(|()| Msg::ShowList);
            Closure::<dyn FnMut(_)>::new(move |_event| {
                callback.emit(());
            })
        };

        leaflet_map.on("draw:created", create_handler.as_ref());
        leaflet_map.on("draw:edited", update_handler.as_ref());
        leaflet_map.on("show_locations", show_list_handler.as_ref());

        Button::new(&ButtonOptions::default()).add_to(&leaflet_map);

        add_tile_layer(&leaflet_map);

        // Hack: Trigger a resize event to force the map to render
        web_sys::window()
            .unwrap()
            .dispatch_event(&Event::new("resize").unwrap())
            .unwrap();

        let tick_callback = ctx.link().callback(|()| Msg::Tick);
        let tick_interval = Interval::new(60_000, move || tick_callback.emit(()));

        Self {
            map: leaflet_map,
            object: MapObject::None,
            container,
            draw_layer,
            draw_control,
            _create_handler: create_handler,
            _update_handler: update_handler,
            _show_locations_handler: show_list_handler,
            tracked_subscription: SubscriptionStatus::Unsubscribed,
            event_subscription: None,
            tracked_objects: HashMap::new(),
            connected: Connected::Disconnected {
                reason: "Loading...".to_string(),
            },
            _tick_interval: tick_interval,
        }
        .tap_mut(|s| Self::set_object(s, object))
        .tap(Self::position_map)
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {}
    }

    #[allow(clippy::cognitive_complexity)]
    #[allow(clippy::too_many_lines)]
    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let props = ctx.props();
        match msg {
            Msg::TrackedObject(topic, location) => {
                let lat_lng = LatLng::new(location.latitude, location.longitude);
                if let Some((existing, marker)) = self.tracked_objects.get_mut(&topic) {
                    marker.set_lat_lng(&lat_lng);
                    update_tooltip(marker, &location);
                    *existing = location;
                } else {
                    let marker = leaflet::Marker::new(&lat_lng);
                    marker.add_to(&self.map);
                    make_tooltip(&marker, &location);
                    self.tracked_objects.insert(topic, (location, marker));
                }
                self.update_zone_styles();
                false
            }
            Msg::SubscribedTracked(subscription) => {
                // If tracked_subscription is unsubscribed, we lost interest in this subscription.
                // If it is in progress, we are waiting for the user to be set.
                // It should never be subscribed, but we handle it just in case.
                if matches!(self.tracked_subscription, SubscriptionStatus::InProgress) {
                    self.tracked_subscription = SubscriptionStatus::Subscribed(subscription);
                }
                false
            }
            Msg::SubscribedEvents(subscription) => {
                self.event_subscription = Some(subscription);
                false
            }
            Msg::MqttEvent(WsEvent::Connected { .. }) => {
                let is_subscribed =
                    matches!(self.tracked_subscription, SubscriptionStatus::Subscribed(_));

                if !is_subscribed {
                    subscribe_to_tracked_objects(ctx);
                    self.tracked_subscription = SubscriptionStatus::InProgress;
                }

                self.connected = Connected::Connected;
                true
            }
            Msg::MqttEvent(WsEvent::Disconnected(reason)) => {
                self.tracked_subscription = SubscriptionStatus::Unsubscribed;
                for (_, (_, marker)) in self.tracked_objects.drain() {
                    marker.remove_from(&self.map);
                }
                self.update_zone_styles();
                self.connected = Connected::Disconnected { reason };
                true
            }
            Msg::CreatePolygon(polygon) => {
                let exterior = polygon
                    .get_lat_lngs()
                    .iter()
                    .flat_map(|lat_lng_array| {
                        let lat_lng_array = lat_lng_array.dyn_into::<js_sys::Array>().unwrap();
                        lat_lng_array
                            .iter()
                            .map(|lat_lng| {
                                let lat_lng = lat_lng.unchecked_into::<leaflet::LatLng>();
                                coord! {x: lat_lng.lng(), y: lat_lng.lat()}
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
                    .pipe(geo::LineString::from);

                let zone = CreateZone {
                    name: "New Zone".to_string(),
                    bounds: geo::Polygon::new(exterior, vec![]),
                    color: "black".to_string(),
                    announce_on_enter: false,
                    announce_on_exit: false,
                };

                props.create_zone.emit(zone);
                false
            }
            Msg::UpdatePolygon(polygon) => {
                let id = self.draw_layer.get_layer_id(&polygon);
                let zone = self.object.get_action_zone_by_id(id);

                if let Some(zone) = zone {
                    let exterior = polygon
                        .get_lat_lngs()
                        .iter()
                        .flat_map(|lat_lng_array| {
                            let lat_lng_array = lat_lng_array.dyn_into::<js_sys::Array>().unwrap();
                            lat_lng_array
                                .iter()
                                .map(|lat_lng| {
                                    let lat_lng = lat_lng.unchecked_into::<leaflet::LatLng>();
                                    coord! {x: lat_lng.lng(), y: lat_lng.lat()}
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                        .pipe(geo::LineString::from);

                    let new_bounds = geo::Polygon::new(exterior, vec![]);
                    // let updates = UpdateZone::Bounds(new_bounds);

                    let mut zone = zone;
                    zone.set_bounds(new_bounds);
                    props.save_zone.emit(zone.clone());
                }
                false
            }
            Msg::UpdateZone(updates) => {
                if let MapObject::Item(zone) = &mut self.object {
                    let mut zone = zone.zone.clone();
                    updates.apply_to_zone(&mut zone);
                    props.update_zone.emit(zone.clone());
                }
                false
            }
            Msg::ShowList => {
                if let MapObject::List(_, _, show_locations) = &mut self.object {
                    if props.loading_status == LoadingStatus::Loaded {
                        *show_locations = true;
                    }
                }
                true
            }
            Msg::SaveZone => {
                if let MapObject::Item(zone) = &self.object {
                    props.save_zone.emit(zone.zone.clone());
                }
                false
            }
            Msg::DeleteItemZone => {
                if let MapObject::Item(zone) = &self.object {
                    let confirmed = web_sys::window()
                        .and_then(|w| {
                            w.confirm_with_message("Are you sure you want to delete this zone?")
                                .ok()
                        })
                        .unwrap_or(false);
                    if confirmed {
                        props.delete_zone.emit(zone.zone.clone());
                    }
                }
                false
            }
            Msg::CancelZone => {
                if let MapObject::Item(_) = &self.object {
                    props.request_list.emit(());
                }
                false
            }
            Msg::CancelList => {
                if let MapObject::List(_, _, show_locations) = &mut self.object {
                    *show_locations = false;
                }
                true
            }
            Msg::Tick => {
                for (location, marker) in self.tracked_objects.values() {
                    update_tooltip(marker, location);
                }
                false
            }
            Msg::SelectZone(zone) => {
                props.request_item.emit(zone);
                false
            }
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.object != old_props.object {
            self.set_object(&props.object);
            self.position_map();

            match &props.object {
                ParamObject::Item(_) => {
                    self.map.remove_control(&self.draw_control);
                }
                ParamObject::List(_) => {
                    self.map.add_control(&self.draw_control);
                }
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();

        let classes = classes!("map-container", "component-container");
        let status = &ctx.props().status;
        let update_zone = ctx.link().callback(Msg::UpdateZone);
        let on_save = ctx.link().callback(|()| Msg::SaveZone);
        let on_cancel_zone = ctx.link().callback(|()| Msg::CancelZone);
        let on_delete_zone = ctx.link().callback(|()| Msg::DeleteItemZone);
        let on_cancel_list = ctx.link().callback(|()| Msg::CancelList);
        let select_zone = ctx.link().callback(Msg::SelectZone);

        let mut messages = vec![];

        match &props.loading_status {
            LoadingStatus::Error(err) => messages.push(err.clone()),
            LoadingStatus::Loading => messages.push("Loading locations...".to_string()),
            LoadingStatus::Loaded => {}
        }

        match &props.status {
            ZoneStatus::Unchanged => {}
            ZoneStatus::Changed => messages.push("Changed".to_string()),
            ZoneStatus::Saving => messages.push("Saving".to_string()),
            ZoneStatus::Error(err) => messages.push(format!("Error: {err}")),
        }

        if let Connected::Disconnected { reason } = &self.connected {
            messages.push(format!("Disconnected: {reason}"));
        }

        let status_msg = if messages.is_empty() {
            None
        } else {
            Some(messages.join(", "))
        };

        let controls = match &self.object {
            MapObject::List(zones, _, true) => {
                html! {
                    <div class="list">
                        <List
                            select_zone={select_zone}
                            zones={zones.clone()}
                            cancel={on_cancel_list}
                        />
                        if let Some(status_msg) = status_msg {
                            {status_msg}
                        }
                    </div>
                }
            }
            MapObject::List(_, _, false) => {
                html! {
                    if let Some(status_msg) = status_msg {
                        <div class="status">
                            {status_msg}
                        </div>
                    }
                }
            }
            MapObject::Item(zone) => {
                html! {
                    <div class="editor">
                        <EditorView
                            zone={zone.zone.clone()}
                            status={status.clone()}
                            update_zone={update_zone}
                            on_save={on_save}
                            on_delete={on_delete_zone}
                            on_cancel={on_cancel_zone}
                        />
                    </div>
                }
            }
            MapObject::None => html! {
                if let Some(status_msg) = status_msg {
                    <div class="status">
                        {status_msg}
                    </div>
                }
            },
        };

        html! {
            <div class={classes}>
                {self.render_map()}
                {controls}
            </div>
        }
    }
}

fn subscribe_to_tracked_objects(ctx: &Context<MapComponent>) {
    let (wss, _): (WebsocketService, _) = ctx
        .link()
        .context(ctx.link().batch_callback(|_| None))
        .unwrap();

    let topic = "robotica/state/+/locations".to_string();
    let callback = ctx.link().callback(move |msg: MqttMessage| {
        let topic = msg.topic.clone();
        let Json(location): Json<LocationMessage> = msg.try_into().unwrap();
        Msg::TrackedObject(topic, location)
    });
    let mut wss = wss;
    ctx.link().send_future(async move {
        let s = wss.subscribe_mqtt(topic, callback).await;
        Msg::SubscribedTracked(s)
    });
}

fn draw_control(draw_layer: &leaflet::FeatureGroup) -> draw_control::DrawControl {
    let edit_options = draw_control::EditOptions::new();
    edit_options.set_feature_group(draw_layer.clone());
    edit_options.set_remove(false);

    let draw_options = draw_control::DrawOptions::new();
    draw_options.set_polyline(false);
    draw_options.set_polygon(true);
    draw_options.set_rectangle(false);
    draw_options.set_circle(false);
    draw_options.set_marker(false);
    draw_options.set_circle_marker(false);

    let options = draw_control::DrawControlOptions::new();
    options.set_draw(draw_options);
    options.set_edit(edit_options);

    draw_control::DrawControl::new(&options)
}

fn create_handler(ctx: &Context<MapComponent>) -> Closure<dyn FnMut(leaflet::Event)> {
    debug!("create_handler");
    let create_polygon = ctx.link().callback(Msg::CreatePolygon);

    Closure::<dyn FnMut(_)>::new(move |event: leaflet::Event| {
        let polygon = event.layer().unchecked_into::<leaflet::Polygon>();
        create_polygon.emit(polygon);
    })
}

fn update_handler(ctx: &Context<MapComponent>) -> Closure<dyn FnMut(leaflet::Event)> {
    debug!("update_handler");
    let update_polygon = ctx.link().callback(Msg::UpdatePolygon);

    Closure::<dyn FnMut(_)>::new(move |event: leaflet::Event| {
        let layers = event
            .pipe(|x| Reflect::get(&x, &"layers".into()))
            .unwrap()
            .unchecked_into::<leaflet::LayerGroup>()
            .get_layers();

        for layer in layers {
            // let layer: leaflet::Polygon = layer.dyn_into().unwrap();
            let layer: leaflet::Polygon = layer.unchecked_into();
            update_polygon.emit(layer);
        }
    })
}

fn add_tile_layer(map: &Map) {
    TileLayer::new("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png").add_to(map);
}

fn tooltip_text(location: &LocationMessage) -> String {
    let minutes_ago = Utc::now()
        .signed_duration_since(location.timestamp)
        .num_minutes();
    let zones: Vec<&str> = location.zones.iter().map(|z| z.name.as_str()).collect();
    if zones.is_empty() {
        format!("{}\n{} min ago", location.label, minutes_ago)
    } else {
        format!(
            "{}\n{} min ago\n{}",
            location.label,
            minutes_ago,
            zones.join(", ")
        )
    }
}

fn make_tooltip(marker: &leaflet::Marker, location: &LocationMessage) {
    let options = leaflet::TooltipOptions::default();
    options.set_sticky(true);
    let tooltip = leaflet::Tooltip::new(&options, None);
    let text = tooltip_text(location);
    tooltip.set_content(&JsValue::from_str(&text));
    marker
        .unchecked_ref::<leaflet::Layer>()
        .bind_tooltip(&tooltip);
}

fn update_tooltip(marker: &leaflet::Marker, location: &LocationMessage) {
    let text = tooltip_text(location);
    marker
        .unchecked_ref::<leaflet::Layer>()
        .set_tooltip_content(&JsValue::from_str(&text));
}
