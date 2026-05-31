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
use leaflet::{Evented, LatLng, Map, MapOptions, TileLayer};
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
    CreatePolyline(leaflet::Polyline),
    UpdateZone(UpdateZone),
    SelectZone(Zone),
    SelectZoneReadOnly(Zone),
    ShowList,
    SaveZone,
    DeleteItemZone,
    CancelZone,
    CancelList,
    Tick,
    ToggleEditBounds,
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
    ReadOnlyItem(Zone),
    None,
}

impl MapObject {}

enum SubscriptionStatus {
    InProgress,
    #[allow(dead_code)]
    Subscribed(Subscription),
    Unsubscribed,
}

#[derive(PartialEq, Eq)]
enum ActiveControl {
    None,
    DrawEdit,
}

pub struct MapComponent {
    map: Map,
    object: MapObject,
    container: HtmlElement,
    draw_layer: leaflet::FeatureGroup,
    draw_control: draw_control::DrawControl,
    measurement_layer: leaflet::FeatureGroup,
    _create_handler: Closure<dyn FnMut(leaflet::Event)>,
    _show_locations_handler: Closure<dyn FnMut(leaflet::Event)>,
    zone_click_handlers: Vec<leaflet::EventedHandle<leaflet::MouseEvent>>,
    tracked_subscription: SubscriptionStatus,
    event_subscription: Option<Subscription>,
    tracked_objects: HashMap<String, (LocationMessage, leaflet::Marker)>,
    connected: Connected,
    is_admin: bool,
    active_control: ActiveControl,
    editing_bounds: bool,
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

    fn set_list(
        &mut self,
        zones: &Arc<Vec<Zone>>,
        select_zone: &Callback<Zone>,
        select_zone_read_only: &Callback<Zone>,
    ) {
        self.draw_layer.clear_layers();
        self.zone_click_handlers.clear();
        let marked_zones = self.get_marked_zones();

        let mut list = Vec::with_capacity(zones.len());
        for zone in zones.iter() {
            let options = get_zone_options(&marked_zones, zone);

            let lat_lngs = zone
                .bounds
                .exterior()
                .coords()
                .map(|lat_lng| LatLng::new(lat_lng.y, lat_lng.x))
                .map(JsValue::from)
                .collect();

            let polygon = leaflet::Polygon::new_with_options(&lat_lngs, &options);

            let zone_clone = zone.clone();
            let is_admin = self.is_admin;
            let cb_admin = select_zone.clone();
            let cb_ro = select_zone_read_only.clone();
            let handle = Evented::on_leaflet_event(
                &&polygon,
                "click",
                move |_event: leaflet::MouseEvent| {
                    if is_admin {
                        cb_admin.emit(zone_clone.clone());
                    } else {
                        cb_ro.emit(zone_clone.clone());
                    }
                },
            );
            self.zone_click_handlers.push(handle);

            polygon
                .unchecked_ref::<leaflet::Layer>()
                .add_to_layer_group(&self.draw_layer);
            let id = self
                .draw_layer
                .get_layer_id(polygon.unchecked_ref::<leaflet::Layer>());

            list.push(MapZone {
                zone: zone.clone(),
                leaflet_id: id,
            });
        }

        self.object = MapObject::List(zones.clone(), list, false);
    }

    fn sync_draw_control(&mut self, ctx: &Context<Self>) {
        let want = self.is_admin && matches!(ctx.props().object, ParamObject::List(_));
        let want = if want {
            ActiveControl::DrawEdit
        } else {
            ActiveControl::None
        };

        if want == self.active_control {
            return;
        }

        if self.active_control == ActiveControl::DrawEdit {
            self.map.remove_control(&self.draw_control);
        }

        if want == ActiveControl::DrawEdit {
            self.map.add_control(&self.draw_control);
        }

        self.active_control = want;
    }

    fn enable_polygon_editing(&self) {
        if let MapObject::Item(ref map_zone) = &self.object {
            let layer = self.draw_layer.get_layer(map_zone.leaflet_id);
            let polygon = layer.unchecked_into::<leaflet::Polygon>();
            Self::enable_editing_on_polygon(&polygon);
        }
    }

    fn disable_polygon_editing(&self) {
        if let MapObject::Item(ref map_zone) = &self.object {
            let layer = self.draw_layer.get_layer(map_zone.leaflet_id);
            let polygon = layer.unchecked_into::<leaflet::Polygon>();
            Self::disable_editing_on_polygon(&polygon);
        }
    }

    fn enable_editing_on_polygon(polygon: &leaflet::Polygon) {
        let window = web_sys::window().unwrap();
        let l_obj = js_sys::Reflect::get(&window, &JsValue::from_str("L")).unwrap();
        let edit = js_sys::Reflect::get(&l_obj, &JsValue::from_str("Edit")).unwrap();
        let poly_ctor = js_sys::Reflect::get(&edit, &JsValue::from_str("Poly")).unwrap();

        let options = js_sys::Object::new();
        let selected_path_options = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &selected_path_options,
            &JsValue::from_str("maintainColor"),
            &JsValue::TRUE,
        );
        let _ = js_sys::Reflect::set(
            &options,
            &JsValue::from_str("selectedPathOptions"),
            &selected_path_options,
        );

        let edit_handler = js_sys::Reflect::construct(
            &poly_ctor.unchecked_into::<js_sys::Function>(),
            &js_sys::Array::of2(polygon.as_ref(), &options),
        )
        .unwrap();

        let _ = js_sys::Reflect::set(
            polygon.as_ref(),
            &JsValue::from_str("editing"),
            &edit_handler,
        );

        let editing =
            js_sys::Reflect::get(polygon.as_ref(), &JsValue::from_str("editing")).unwrap();
        let enable_fn = js_sys::Reflect::get(&editing, &JsValue::from_str("enable"))
            .unwrap()
            .unchecked_into::<js_sys::Function>();
        let _ = js_sys::Reflect::apply(&enable_fn, &editing, &js_sys::Array::new());
    }

    fn disable_editing_on_polygon(polygon: &leaflet::Polygon) {
        if let Ok(editing) = js_sys::Reflect::get(polygon.as_ref(), &JsValue::from_str("editing")) {
            if !editing.is_undefined() && !editing.is_null() {
                let disable_fn = js_sys::Reflect::get(&editing, &JsValue::from_str("disable"))
                    .unwrap()
                    .unchecked_into::<js_sys::Function>();
                let _ = js_sys::Reflect::apply(&disable_fn, &editing, &js_sys::Array::new());
            }
        }
    }

    fn capture_bounds_from_map(&mut self, leaflet_id: i32) {
        let layer = self.draw_layer.get_layer(leaflet_id);
        let polygon: leaflet::Polygon = layer.unchecked_into();
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
        if let MapObject::Item(ref mut map_zone) = &mut self.object {
            map_zone.zone.set_bounds(new_bounds);
        }
    }

    fn get_marked_zones(&self) -> Vec<i32> {
        self.tracked_objects
            .values()
            .flat_map(|(loc, _)| loc.zones.iter().map(|z| z.id))
            .collect()
    }

    fn set_object(
        &mut self,
        object: &ParamObject,
        select_zone: &Callback<Zone>,
        select_zone_read_only: &Callback<Zone>,
    ) {
        if self.editing_bounds {
            self.disable_polygon_editing();
            self.editing_bounds = false;
        }
        match object {
            ParamObject::List(zones) => {
                self.set_list(zones, select_zone, select_zone_read_only);
            }
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
            MapObject::ReadOnlyItem(_) | MapObject::None => {}
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
            MapObject::Item(_) | MapObject::ReadOnlyItem(_) => {
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

fn is_same_zone_item(a: &ParamObject, b: &ParamObject) -> bool {
    matches!((a, b), (ParamObject::Item(a), ParamObject::Item(b)) if a.id() == b.id())
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

        let measurement_layer = leaflet::FeatureGroup::new();
        measurement_layer.add_to(&leaflet_map);

        let draw_control = draw_control();

        let create_handler = create_handler(ctx);
        let show_list_handler = {
            let callback = ctx.link().callback(|()| Msg::ShowList);
            Closure::<dyn FnMut(_)>::new(move |_event| {
                callback.emit(());
            })
        };

        leaflet_map.on("draw:created", create_handler.as_ref());
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
            measurement_layer,
            _create_handler: create_handler,
            _show_locations_handler: show_list_handler,
            zone_click_handlers: Vec::new(),
            tracked_subscription: SubscriptionStatus::Unsubscribed,
            event_subscription: None,
            tracked_objects: HashMap::new(),
            connected: Connected::Disconnected {
                reason: "Loading...".to_string(),
            },
            is_admin: false,
            active_control: ActiveControl::None,
            editing_bounds: false,
            _tick_interval: tick_interval,
        }
        .tap_mut(|s| {
            let select_zone = ctx.link().callback(Msg::SelectZone);
            let select_zone_ro = ctx.link().callback(Msg::SelectZoneReadOnly);
            Self::set_object(s, object, &select_zone, &select_zone_ro);
        })
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
            Msg::MqttEvent(WsEvent::Connected { user, .. }) => {
                let is_subscribed = matches!(
                    self.tracked_subscription,
                    SubscriptionStatus::Subscribed(_) | SubscriptionStatus::InProgress
                );

                if !is_subscribed {
                    subscribe_to_tracked_objects(ctx);
                    self.tracked_subscription = SubscriptionStatus::InProgress;
                }

                let was_admin = self.is_admin;
                self.is_admin = user.is_admin;
                if !was_admin && self.is_admin {
                    self.sync_draw_control(ctx);
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
                self.is_admin = false;
                if self.active_control == ActiveControl::DrawEdit {
                    self.map.remove_control(&self.draw_control);
                }
                self.active_control = ActiveControl::None;
                self.connected = Connected::Disconnected { reason };
                true
            }
            Msg::MqttEvent(WsEvent::LoginRequired { .. }) => {
                self.tracked_subscription = SubscriptionStatus::Unsubscribed;
                for (_, (_, marker)) in self.tracked_objects.drain() {
                    marker.remove_from(&self.map);
                }
                self.update_zone_styles();
                self.is_admin = false;
                if self.active_control == ActiveControl::DrawEdit {
                    self.map.remove_control(&self.draw_control);
                }
                self.active_control = ActiveControl::None;
                self.connected = Connected::Disconnected {
                    reason: "Login required".to_string(),
                };
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
            Msg::CreatePolyline(polyline) => {
                let lat_lngs = polyline.get_lat_lngs();
                let points: Vec<leaflet::LatLng> = lat_lngs
                    .iter()
                    .flat_map(|inner| {
                        let inner = inner.dyn_into::<js_sys::Array>().unwrap();
                        inner
                            .iter()
                            .map(wasm_bindgen::JsCast::unchecked_into::<leaflet::LatLng>)
                            .collect::<Vec<_>>()
                    })
                    .collect();

                let total_distance = if points.len() >= 2 {
                    let mut total = 0.0;
                    for i in 1..points.len() {
                        total += self.map.distance(&points[i - 1], &points[i]);
                    }
                    total
                } else {
                    0.0
                };

                let polyline_options = leaflet::PolylineOptions::default();
                polyline_options.set_color("blue".to_string());
                polyline_options.set_weight(3.0);
                polyline_options.set_dash_array("10, 10".to_string());
                polyline.set_style(&polyline_options);

                let tooltip_options = leaflet::TooltipOptions::default();
                tooltip_options.set_permanent(true);
                tooltip_options.set_direction("center".to_string());
                let tooltip = leaflet::Tooltip::new(&tooltip_options, None);
                let text = if total_distance >= 1000.0 {
                    format!("{:.2} km", total_distance / 1000.0)
                } else {
                    format!("{total_distance:.1} m")
                };
                tooltip.set_content(&JsValue::from_str(&text));

                let layer: &leaflet::Layer = polyline.unchecked_ref();
                layer.bind_tooltip(&tooltip);
                let center = polyline.get_center();
                layer.open_tooltip(&center);

                layer.add_to_layer_group(&self.measurement_layer);

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
                let leaflet_id = match &self.object {
                    MapObject::Item(zone) => zone.leaflet_id,
                    _ => return false,
                };
                self.capture_bounds_from_map(leaflet_id);
                if let MapObject::Item(ref map_zone) = self.object {
                    props.save_zone.emit(map_zone.zone.clone());
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
                if self.editing_bounds {
                    self.disable_polygon_editing();
                    self.editing_bounds = false;
                }
                if let MapObject::Item(_) | MapObject::ReadOnlyItem(_) = &self.object {
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
            Msg::ToggleEditBounds => {
                self.editing_bounds = !self.editing_bounds;
                if self.editing_bounds {
                    self.enable_polygon_editing();
                } else {
                    self.disable_polygon_editing();
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
            Msg::SelectZoneReadOnly(zone) => {
                self.object = MapObject::ReadOnlyItem(zone);
                true
            }
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.object != old_props.object {
            if !is_same_zone_item(&props.object, &old_props.object) {
                let select_zone = ctx.link().callback(Msg::SelectZone);
                let select_zone_ro = ctx.link().callback(Msg::SelectZoneReadOnly);
                self.set_object(&props.object, &select_zone, &select_zone_ro);

                let skip = match (&old_props.object, &props.object) {
                    (ParamObject::Item(_), _) => true,
                    (ParamObject::List(old), ParamObject::List(_)) => !old.is_empty(),
                    _ => false,
                };
                if !skip {
                    self.position_map();
                }
            } else if let (ParamObject::Item(new_zone), MapObject::Item(ref mut existing)) =
                (&props.object, &mut self.object)
            {
                existing.zone = new_zone.clone();
            }
        }

        self.sync_draw_control(ctx);

        true
    }

    #[allow(clippy::too_many_lines)]
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
            ZoneStatus::Idle => {}
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
                let on_edit_bounds = ctx.link().callback(|()| Msg::ToggleEditBounds);
                html! {
                    <div class="editor">
                        <EditorView
                            zone={zone.zone.clone()}
                            status={status.clone()}
                            update_zone={update_zone}
                            on_save={on_save}
                            on_delete={on_delete_zone}
                            on_cancel={on_cancel_zone}
                            on_edit_bounds={on_edit_bounds}
                        />
                    </div>
                }
            }
            MapObject::ReadOnlyItem(zone) => {
                let on_close = ctx.link().callback(|_| Msg::CancelZone);
                let color_swatch = format!("background-color: {};", zone.color);
                html! {
                    <div class="editor">
                        <h1>{&zone.name}</h1>
                        <div class="read-only-details">
                            <p>
                                <span class="color-swatch" style={color_swatch}></span>
                                {" "}{&zone.color}
                            </p>
                            <p>{if zone.announce_on_enter { "Announces on enter" } else { "Silent on enter" }}</p>
                            <p>{if zone.announce_on_exit { "Announces on exit" } else { "Silent on exit" }}</p>
                        </div>
                        <button onclick={on_close}>{"Close"}</button>
                        if let Some(status_msg) = status_msg {
                            <p>{status_msg}</p>
                        }
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

fn draw_control() -> draw_control::DrawControl {
    let draw_options = draw_control::DrawOptions::new();
    draw_options.set_polyline(true);
    draw_options.set_polygon(true);
    draw_options.set_rectangle(false);
    draw_options.set_circle(false);
    draw_options.set_marker(false);
    draw_options.set_circle_marker(false);

    let options = draw_control::DrawControlOptions::new();
    options.set_draw(draw_options);

    draw_control::DrawControl::new(&options)
}

fn create_handler(ctx: &Context<MapComponent>) -> Closure<dyn FnMut(leaflet::Event)> {
    debug!("create_handler");
    let create_polygon = ctx.link().callback(Msg::CreatePolygon);
    let create_polyline = ctx.link().callback(Msg::CreatePolyline);

    Closure::<dyn FnMut(_)>::new(move |event: leaflet::Event| {
        let layer = event.layer();
        if let Ok(polyline) = layer.clone().dyn_into::<leaflet::Polyline>() {
            create_polyline.emit(polyline);
        } else {
            let polygon = layer.unchecked_into::<leaflet::Polygon>();
            create_polygon.emit(polygon);
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
