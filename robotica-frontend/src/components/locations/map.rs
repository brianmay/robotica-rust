use std::sync::Arc;

use crate::{
    components::locations::{editor::EditorView, list::List},
    robotica_wasm::{
        draw_control,
        robotica::{Button, ButtonOptions},
    },
    services::websocket::{Subscription, WebsocketService, WsEvent},
};
use geo::coord;
use gloo_utils::document;
use itertools::Itertools;
use js_sys::Reflect;
use leaflet::{LatLng, Map, MapOptions, TileLayer};
use robotica_common::{
    mqtt::{Json, MqttMessage},
    robotica::locations::{CreateLocation, Location, LocationMessage},
    user::User,
};
use tap::{Pipe, Tap};
use tracing::debug;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{Element, HtmlElement, Node};
use yew::prelude::*;

use super::{
    editor::UpdateLocation,
    locations_view::{LoadingStatus, LocationStatus},
    ActionLocation,
};

pub enum Msg {
    Car(LocationMessage),
    SubscribedCar(Subscription),
    SubscribedEvents(Subscription),
    MqttEvent(WsEvent),
    CreatePolygon(leaflet::Polygon),
    UpdatePolygon(leaflet::Polygon),
    DeletePolygon(leaflet::Polygon),
    UpdateLocation(UpdateLocation),
    SelectLocation(Location),
    ShowList,
    SaveLocation,
    CancelLocation,
    CancelList,
}

#[derive(PartialEq, Clone)]
pub enum ParamObject {
    List(Arc<Vec<Location>>),
    Item(ActionLocation),
}

struct MapLocation {
    location: Location,
    leaflet_id: i32,
}

struct MapActionLocation {
    location: ActionLocation,
    leaflet_id: i32,
}

enum MapObject {
    List(Arc<Vec<Location>>, Vec<MapLocation>, bool),
    Item(MapActionLocation),
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

    fn get_action_location_by_id(&self, id: i32) -> Option<ActionLocation> {
        match self {
            MapObject::List(_, locations, _) => locations
                .iter()
                .find(|location| location.leaflet_id == id)
                .map(|location| ActionLocation::Update(location.location.clone())),

            MapObject::Item(location) => {
                if location.leaflet_id == id {
                    Some(location.location.clone())
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
    Subscribed(Subscription),
    Unsubscribed,
}

pub struct MapComponent {
    map: Map,
    user: Option<User>,
    object: MapObject,
    container: HtmlElement,
    draw_layer: leaflet::FeatureGroup,
    _create_handler: Closure<dyn FnMut(leaflet::Event)>,
    _update_handler: Closure<dyn FnMut(leaflet::Event)>,
    _delete_handler: Closure<dyn FnMut(leaflet::Event)>,
    _show_locations_handler: Closure<dyn FnMut(leaflet::Event)>,
    car_subscription: SubscriptionStatus,
    event_subscription: Option<Subscription>,
    car_marker: Option<leaflet::Marker>,
    car: Option<LocationMessage>,
    connected: bool,
}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub object: ParamObject,
    pub create_location: Callback<CreateLocation>,
    pub update_location: Callback<ActionLocation>,
    pub delete_location: Callback<ActionLocation>,
    pub save_location: Callback<ActionLocation>,
    pub request_item: Callback<Location>,
    pub request_list: Callback<()>,
    pub status: LocationStatus,
    pub loading_status: LoadingStatus,
}

impl MapComponent {
    fn render_map(&self) -> Html {
        let node: &Node = &self.container.clone().into();
        Html::VRef(node.clone())
    }

    fn set_item(&mut self, location: ActionLocation) {
        self.draw_layer.clear_layers();
        let marked_locations = self.get_marked_locations();
        let options = get_action_location_options(&marked_locations, &location);

        self.draw_layer.clear_layers();
        let lat_lngs = location
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

        self.object = MapObject::Item(MapActionLocation {
            location,
            leaflet_id: id,
        });
    }

    fn set_list(&mut self, locations: &Arc<Vec<Location>>) {
        self.draw_layer.clear_layers();
        let marked_locations = self.get_marked_locations();

        let list: Vec<MapLocation> = locations
            .iter()
            .map(|location| {
                let options = get_location_options(&marked_locations, location);

                let lat_lngs = location
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

                MapLocation {
                    location: location.clone(),
                    leaflet_id: id,
                }
            })
            .collect();

        self.object = MapObject::List(locations.clone(), list, false);
    }

    fn get_marked_locations(&self) -> Vec<&Location> {
        let no_locations = vec![];
        if let Some(car) = &self.car {
            car.locations.iter().collect_vec()
        } else {
            no_locations
        }
    }

    fn set_object(&mut self, object: &ParamObject) {
        match object {
            ParamObject::List(locations) => self.set_list(locations),
            ParamObject::Item(location) => self.set_item(location.clone()),
        }
    }

    fn iterate_over_layers(&self, f: impl Fn(&ActionLocation, leaflet::Layer)) {
        match &self.object {
            MapObject::List(_, locations, _) => {
                for location in locations {
                    let layer = self.draw_layer.get_layer(location.leaflet_id);
                    let location = ActionLocation::Update(location.location.clone());
                    f(&location, layer);
                }
            }
            MapObject::Item(location) => {
                let id = location.leaflet_id;
                let layer = self.draw_layer.get_layer(id);
                f(&location.location, layer);
            }
            MapObject::None => {}
        }
    }

    fn update_location_styles(&self) {
        let marked_locations = self.get_marked_locations();

        self.iterate_over_layers(|location, layer| {
            // let layer: leaflet::Polygon = layer.dyn_into().unwrap();
            let layer = layer.unchecked_into::<leaflet::Polyline>();
            let options = get_action_location_options(&marked_locations, location);
            layer.set_style(&options);
        });
    }

    #[allow(clippy::cognitive_complexity)]
    fn position_map(&self) {
        match &self.object {
            MapObject::None => {
                self.map.fit_world();
            }
            MapObject::List(_, locations, _) => {
                if locations.is_empty() {
                    self.map.fit_world();
                } else {
                    self.map.fit_bounds(self.draw_layer.get_bounds().as_ref());
                }
            }
            MapObject::Item(_location) => {
                self.map.fit_bounds(self.draw_layer.get_bounds().as_ref());
            }
        }
    }
}

fn get_action_location_options(
    marked_locations: &[&Location],
    location: &ActionLocation,
) -> leaflet::PolylineOptions {
    let color = get_action_location_color(marked_locations, location);
    let options = leaflet::PolylineOptions::default();
    options.set_color(color.clone());
    options.set_fill_color(color);
    options.set_weight(3.0);
    options.set_opacity(0.5);
    options.set_fill(true);
    options
}

fn get_location_options(
    marked_locations: &[&Location],
    location: &Location,
) -> leaflet::PolylineOptions {
    let color = get_location_color(marked_locations, location);
    let options = leaflet::PolylineOptions::default();
    options.set_color(color.clone());
    options.set_fill_color(color);
    options.set_weight(3.0);
    options.set_opacity(0.5);
    options.set_fill(true);
    options
}

fn get_action_location_color(marked_locations: &[&Location], location: &ActionLocation) -> String {
    match location {
        ActionLocation::Create(_location) => "black".to_string(),
        ActionLocation::Update(location) => get_location_color(marked_locations, location),
    }
}

fn get_location_color(marked_locations: &[&Location], location: &Location) -> String {
    let is_marked = marked_locations
        .iter()
        .any(|marked_location| location.id == marked_location.id);

    if is_marked {
        "red".to_string()
    } else {
        location.color.clone()
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
        let leaflet_map = Map::new_with_element(&container, &MapOptions::default());

        let draw_layer = leaflet::FeatureGroup::new();
        draw_layer.add_to(&leaflet_map);

        let draw_control = draw_control(&draw_layer);
        leaflet_map.add_control(&draw_control);

        let create_handler = create_handler(ctx);
        let update_handler = update_handler(ctx);
        let delete_handler = delete_handler(ctx);
        let show_list_handler = {
            let callback = ctx.link().callback(|()| Msg::ShowList);
            Closure::<dyn FnMut(_)>::new(move |_event| {
                callback.emit(());
            })
        };

        leaflet_map.on("draw:created", create_handler.as_ref());
        leaflet_map.on("draw:edited", update_handler.as_ref());
        leaflet_map.on("draw:deleted", delete_handler.as_ref());
        leaflet_map.on("show_locations", show_list_handler.as_ref());

        Button::new(&ButtonOptions::default()).add_to(&leaflet_map);

        add_tile_layer(&leaflet_map);

        // Hack: Trigger a resize event to force the map to render
        web_sys::window()
            .unwrap()
            .dispatch_event(&Event::new("resize").unwrap())
            .unwrap();

        Self {
            map: leaflet_map,
            user: None,
            object: MapObject::None,
            container,
            draw_layer,
            _create_handler: create_handler,
            _update_handler: update_handler,
            _delete_handler: delete_handler,
            _show_locations_handler: show_list_handler,
            car_subscription: SubscriptionStatus::Unsubscribed,
            event_subscription: None,
            car_marker: None,
            car: None,
            connected: false,
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
            Msg::Car(location) => {
                let position = location.position;
                if let Some(ref marker) = self.car_marker {
                    marker.set_lat_lng(&LatLng::new(position.y(), position.x()));
                } else {
                    let car_marker = leaflet::Marker::new(&LatLng::new(position.y(), position.x()));
                    car_marker.add_to(&self.map);
                    self.car_marker = Some(car_marker);
                }
                self.car = Some(location);
                self.update_location_styles();
                false
            }
            Msg::SubscribedCar(subscription) => {
                // If car_subscription is unsubscribed, we lost interest in this subscription.
                // If it is in progress, we are waiting for the user to be set.
                // It should never be subscribed, but we handle it just in case.
                if matches!(self.car_subscription, SubscriptionStatus::InProgress) {
                    self.car_subscription = SubscriptionStatus::Subscribed(subscription);
                }
                false
            }
            Msg::SubscribedEvents(subscription) => {
                self.event_subscription = Some(subscription);
                false
            }
            Msg::MqttEvent(WsEvent::Connected { user, .. }) => {
                let is_subscribed =
                    matches!(self.car_subscription, SubscriptionStatus::Subscribed(_));
                let should_subscribe = user.is_admin;

                if !is_subscribed && should_subscribe {
                    subscribe_to_car(ctx);
                    self.car_subscription = SubscriptionStatus::InProgress;
                } else if !should_subscribe {
                    self.car_subscription = SubscriptionStatus::Unsubscribed;
                }

                self.user = Some(user);
                self.connected = true;
                true
            }
            Msg::MqttEvent(WsEvent::Disconnected(_reason)) => {
                self.user = None;
                self.car_subscription = SubscriptionStatus::Unsubscribed;
                self.car = None;
                if let Some(car_marker) = &self.car_marker {
                    car_marker.remove_from(&self.map);
                }
                self.car_marker = None;
                self.update_location_styles();
                self.connected = false;
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

                let location = CreateLocation {
                    name: "New Location".to_string(),
                    bounds: geo::Polygon::new(exterior, vec![]),
                    color: "black".to_string(),
                    announce_on_enter: false,
                    announce_on_exit: false,
                };

                props.create_location.emit(location);
                false
            }
            Msg::UpdatePolygon(polygon) => {
                let id = self.draw_layer.get_layer_id(&polygon);
                let location = self.object.get_action_location_by_id(id);

                if let Some(location) = location {
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
                    // let updates = UpdateLocation::Bounds(new_bounds);

                    let mut location = location;
                    location.set_bounds(new_bounds);
                    props.save_location.emit(location.clone());
                }
                false
            }
            Msg::DeletePolygon(polygon) => {
                let id = self.draw_layer.get_layer_id(&polygon);
                let location = self.object.get_action_location_by_id(id);
                if let Some(location) = location {
                    props.delete_location.emit(location);
                }
                false
            }
            Msg::UpdateLocation(updates) => {
                if let MapObject::Item(location) = &mut self.object {
                    let mut location = location.location.clone();
                    updates.apply_to_location(&mut location);
                    props.update_location.emit(location.clone());
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
            Msg::SaveLocation => {
                if let MapObject::Item(location) = &self.object {
                    props.save_location.emit(location.location.clone());
                }
                false
            }
            Msg::CancelLocation => {
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
            Msg::SelectLocation(location) => {
                props.request_item.emit(location);
                false
            }
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.object != old_props.object {
            self.set_object(&props.object);
            self.position_map();
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();

        let classes = classes!("map-container", "component-container");
        let status = &ctx.props().status;
        let update_location = ctx.link().callback(Msg::UpdateLocation);
        let on_save = ctx.link().callback(|()| Msg::SaveLocation);
        let on_cancel_location = ctx.link().callback(|()| Msg::CancelLocation);
        let on_cancel_list = ctx.link().callback(|()| Msg::CancelList);
        let select_location = ctx.link().callback(Msg::SelectLocation);
        let connected = self.connected | !self.user.as_ref().map_or(true, |user| user.is_admin);

        let status_msg = match (&props.status, &props.loading_status, connected) {
            (LocationStatus::Unchanged, LoadingStatus::Error(err), _) => {
                format!("LoadingError {err}").pipe(Some)
            }
            (LocationStatus::Unchanged, LoadingStatus::Loading, _) => {
                "Loading".to_string().pipe(Some)
            }
            (LocationStatus::Unchanged, LoadingStatus::Loaded, false) => {
                "Disconnected".to_string().pipe(Some)
            }
            (LocationStatus::Unchanged, LoadingStatus::Loaded, true) => None,
            (LocationStatus::Changed, _, _) => "Changed".to_string().pipe(Some),
            (LocationStatus::Saving, _, _) => "Saving".to_string().pipe(Some),
            (LocationStatus::Error(err), _, _) => format!("Error {err}").pipe(Some),
        };

        let controls = match &self.object {
            MapObject::List(locations, _, true) => {
                html! {
                    <div class="list">
                        <List
                            select_location={select_location}
                            locations={locations.clone()}
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
            MapObject::Item(location) => {
                html! {
                    <div class="editor">
                        <EditorView
                            location={location.location.clone()}
                            status={status.clone()}
                            update_location={update_location}
                            on_save={on_save}
                            on_cancel={on_cancel_location}
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

fn subscribe_to_car(ctx: &Context<MapComponent>) {
    let (wss, _): (WebsocketService, _) = ctx
        .link()
        .context(ctx.link().batch_callback(|_| None))
        .unwrap();

    let topic = "state/Tesla/1/Locations".to_string();
    let callback = ctx.link().callback(move |msg: MqttMessage| {
        let Json(location): Json<LocationMessage> = msg.try_into().unwrap();
        Msg::Car(location)
    });
    let mut wss = wss;
    ctx.link().send_future(async move {
        let s = wss.subscribe_mqtt(topic, callback).await;
        Msg::SubscribedCar(s)
    });
}

fn draw_control(draw_layer: &leaflet::FeatureGroup) -> draw_control::DrawControl {
    let edit_options = draw_control::EditOptions::new();
    edit_options.set_feature_group(draw_layer.clone());

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

fn delete_handler(ctx: &Context<MapComponent>) -> Closure<dyn FnMut(leaflet::Event)> {
    debug!("delete_handler");
    let delete_polygon = ctx.link().callback(Msg::DeletePolygon);

    Closure::<dyn FnMut(_)>::new(move |event: leaflet::Event| {
        let layers = event
            .pipe(|x| Reflect::get(&x, &"layers".into()))
            .unwrap()
            .unchecked_into::<leaflet::LayerGroup>()
            .get_layers();

        for layer in layers {
            // let layer: leaflet::Polygon = layer.dyn_into().unwrap();
            let layer: leaflet::Polygon = layer.unchecked_into();
            delete_polygon.emit(layer);
        }
    })
}

fn add_tile_layer(map: &Map) {
    TileLayer::new("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png").add_to(map);
}
