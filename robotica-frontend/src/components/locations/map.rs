use std::sync::Arc;

use crate::{
    robotica_wasm::draw_control,
    services::websocket::{Subscription, WebsocketService, WsEvent},
};
use geo::coord;
use gloo_utils::document;
use js_sys::Reflect;
use leaflet::{LatLng, Map, MapOptions, TileLayer};
use robotica_common::{
    mqtt::{Json, MqttMessage},
    robotica::locations::{Location, LocationMessage},
    user::User,
};
use tap::{Pipe, Tap};
use tracing::debug;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{Element, HtmlElement, Node};
use yew::prelude::*;

use super::ActionLocation;

pub enum Msg {
    Car(LocationMessage),
    SubscribedCar(Subscription),
    SubscribedEvents(Subscription),
    MqttEvent(WsEvent),
}

#[derive(PartialEq, Clone)]
pub enum MapObject {
    List(Arc<Vec<Location>>),
    Item(ActionLocation),
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
    draw_control: draw_control::DrawControl,
    _create_handler: Closure<dyn FnMut(leaflet::Event)>,
    _update_handler: Closure<dyn FnMut(leaflet::Event)>,
    _delete_handler: Closure<dyn FnMut(leaflet::Event)>,
    car_subscription: SubscriptionStatus,
    event_subscription: Option<Subscription>,
    car_marker: Option<leaflet::Marker>,
    car: Option<LocationMessage>,
}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub object: MapObject,
    pub create_polygon: Callback<geo::Polygon>,
    pub update_polygon: Callback<geo::Polygon>,
    pub delete_polygon: Callback<()>,
}

impl MapComponent {
    fn render_map(&self) -> Html {
        let node: &Node = &self.container.clone().into();
        Html::VRef(node.clone())
    }

    fn draw_item(&self, location: &ActionLocation) {
        self.draw_layer.clear_layers();

        let options = leaflet::PolylineOptions::default();
        options.set_color(location.color());
        options.set_fill_color(location.color());
        options.set_weight(3.0);
        options.set_opacity(0.5);
        options.set_fill(true);

        self.draw_layer.clear_layers();
        let lat_lngs = location
            .bounds()
            .exterior()
            .coords()
            .map(|lat_lng| LatLng::new(lat_lng.y, lat_lng.x))
            .map(JsValue::from)
            .collect();

        leaflet::Polygon::new_with_options(&lat_lngs, &options)
            .unchecked_into::<leaflet::Layer>()
            .add_to_layer_group(&self.draw_layer);

        debug!(
            "Fitting bounds in draw_item {:?}",
            self.draw_layer.get_bounds()
        );
        self.map.fit_bounds(self.draw_layer.get_bounds().as_ref());
    }

    fn draw_list(&self, locations: &Vec<Location>) {
        self.draw_layer.clear_layers();

        let options = leaflet::PolylineOptions::default();
        options.set_weight(3.0);
        options.set_opacity(0.5);
        options.set_fill(true);

        let no_locations = vec![];
        let marked_locations = self
            .car
            .as_ref()
            .map_or(&no_locations, |car| &car.locations);

        for location in locations {
            let is_marked = marked_locations
                .iter()
                .any(|marked_location| location.id == marked_location.id);

            let color = if is_marked {
                "red"
            } else {
                location.color.as_str()
            };

            options.set_color(color.to_string());
            options.set_fill_color(color.to_string());

            let lat_lngs = location
                .bounds
                .exterior()
                .coords()
                .map(|lat_lng| LatLng::new(lat_lng.y, lat_lng.x))
                .map(JsValue::from)
                .collect();

            leaflet::Polygon::new_with_options(&lat_lngs, &options)
                .unchecked_into::<leaflet::Layer>()
                .add_to_layer_group(&self.draw_layer);
        }

        if locations.is_empty() {
            self.map.fit_world();
        } else {
            debug!(
                "Fitting bounds in draw_list {:?}",
                self.draw_layer.get_bounds()
            );
            self.map.fit_bounds(self.draw_layer.get_bounds().as_ref());
        }
    }

    fn draw_object(&self) {
        match &self.object {
            MapObject::List(locations) => self.draw_list(locations),
            MapObject::Item(location) => self.draw_item(location),
        }
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

        let draw_control = draw_control(&draw_layer, object);
        leaflet_map.add_control(&draw_control);

        let create_handler = create_handler(ctx);
        let update_handler = update_handler(ctx);
        let delete_handler = delete_handler(ctx);

        leaflet_map.on("draw:created", create_handler.as_ref());
        leaflet_map.on("draw:edited", update_handler.as_ref());
        leaflet_map.on("draw:deleted", delete_handler.as_ref());

        add_tile_layer(&leaflet_map);

        // Hack: Trigger a resize event to force the map to render
        web_sys::window()
            .unwrap()
            .dispatch_event(&Event::new("resize").unwrap())
            .unwrap();

        Self {
            map: leaflet_map,
            user: None,
            object: object.clone(),
            container,
            draw_layer,
            draw_control,
            _create_handler: create_handler,
            _update_handler: update_handler,
            _delete_handler: delete_handler,
            car_subscription: SubscriptionStatus::Unsubscribed,
            event_subscription: None,
            car_marker: None,
            car: None,
        }
        .tap(Self::draw_object)
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {}
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
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
                self.draw_object();
            }
            Msg::SubscribedCar(subscription) => {
                // If car_subscription is unsubscribed, we lost interest in this subscription.
                // If it is in progress, we are waiting for the user to be set.
                // It should never be subscribed, but we handle it just in case.
                if matches!(self.car_subscription, SubscriptionStatus::InProgress) {
                    self.car_subscription = SubscriptionStatus::Subscribed(subscription);
                }
            }
            Msg::SubscribedEvents(subscription) => {
                self.event_subscription = Some(subscription);
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
            }
            Msg::MqttEvent(WsEvent::Disconnected(_reason)) => {
                self.user = None;
                self.car_subscription = SubscriptionStatus::Unsubscribed;
                self.car = None;
                if let Some(car_marker) = &self.car_marker {
                    car_marker.remove_from(&self.map);
                }
                self.car_marker = None;
                self.draw_object();
            }
        }
        false
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.object == old_props.object {
            return false;
        }
        self.object = props.object.clone();

        match (&props.object, &old_props.object) {
            (MapObject::Item(_), MapObject::Item(_)) | (MapObject::List(_), MapObject::List(_)) => {
            }
            _ => {
                self.map.remove_control(&self.draw_control);
                self.draw_control = draw_control(&self.draw_layer, &props.object);
                self.map.add_control(&self.draw_control);
                self.draw_object();
            }
        }

        false
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <div class="map-container component-container">
                {self.render_map()}
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

fn draw_control(
    draw_layer: &leaflet::FeatureGroup,
    object: &MapObject,
) -> draw_control::DrawControl {
    let options = match object {
        MapObject::List(_locations) => {
            let draw_options = draw_control::DrawOptions::new();
            draw_options.set_polyline(false);
            draw_options.set_polygon(true);
            draw_options.set_rectangle(false);
            draw_options.set_circle(false);
            draw_options.set_marker(false);
            draw_options.set_circle_marker(false);

            let options = draw_control::DrawControlOptions::new();
            options.set_draw(draw_options);

            options
        }
        MapObject::Item(_location) => {
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

            options
        }
    };
    draw_control::DrawControl::new(&options)
}

fn create_handler(ctx: &Context<MapComponent>) -> Closure<dyn FnMut(leaflet::Event)> {
    let create_polygon = ctx.props().create_polygon.clone();
    Closure::<dyn FnMut(_)>::new(move |event: leaflet::Event| {
        let exterior = event
            .layer()
            .unchecked_into::<leaflet::Polyline>()
            .get_lat_lngs()
            .iter()
            .flat_map(|lat_lng_array| {
                let lat_lng_array = lat_lng_array.dyn_into::<js_sys::Array>().unwrap();
                lat_lng_array
                    .iter()
                    .map(|lat_lng| {
                        let lat_lng = lat_lng.unchecked_into::<leaflet::LatLng>();
                        geo::Point(coord! {x: lat_lng.lng(), y: lat_lng.lat()})
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
            .pipe(geo::LineString::from);

        create_polygon.emit(geo::Polygon::new(exterior, vec![]));
    })
}

fn update_handler(ctx: &Context<MapComponent>) -> Closure<dyn FnMut(leaflet::Event)> {
    let update_polygon = ctx.props().update_polygon.clone();
    Closure::<dyn FnMut(_)>::new(move |event: leaflet::Event| {
        let exterior = event
            // .unchecked_into::<JsValue>()
            .pipe(|x| Reflect::get(&x, &"layers".into()))
            .unwrap()
            .unchecked_into::<leaflet::LayerGroup>()
            .get_layers()
            .get(0)
            .unchecked_into::<leaflet::Polyline>()
            .get_lat_lngs()
            .iter()
            .flat_map(|lat_lng_array| {
                let lat_lng_array = lat_lng_array.dyn_into::<js_sys::Array>().unwrap();
                lat_lng_array
                    .iter()
                    .map(|lat_lng| {
                        let lat_lng = lat_lng.unchecked_into::<leaflet::LatLng>();
                        geo::Point(coord! {x: lat_lng.lng(), y: lat_lng.lat()})
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
            .pipe(geo::LineString::from);

        update_polygon.emit(geo::Polygon::new(exterior, vec![]));
    })
}

fn delete_handler(ctx: &Context<MapComponent>) -> Closure<dyn FnMut(leaflet::Event)> {
    let delete_polygon = ctx.props().delete_polygon.clone();
    Closure::<dyn FnMut(_)>::new(move |_event: leaflet::Event| {
        delete_polygon.emit(());
    })
}

fn add_tile_layer(map: &Map) {
    TileLayer::new("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png").add_to(map);
}
