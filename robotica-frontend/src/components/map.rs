use super::locations::LocationWrapper;
use geo::coord;
use gloo_utils::document;
use leaflet::{LatLng, Map, MapOptions, TileLayer};
use std::sync::Arc;
use tap::Tap;
use tracing::debug;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, HtmlElement, Node};
use yew::prelude::*;

pub enum Msg {}

pub struct MapComponent {
    map: Map,
    lat: geo::Point,
    container: HtmlElement,
}

// #[derive(Copy, Clone, Debug, PartialEq)]
// pub struct Point(pub f64, pub f64);

// #[derive(PartialEq, Clone, Debug)]
// pub struct City {
//     pub name: String,
//     pub lat: Point,
// }

// impl ImplicitClone for City {}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub location: Option<LocationWrapper>,
    pub locations: Arc<Vec<LocationWrapper>>,
    pub lat: geo::Point,
}

impl MapComponent {
    fn render_map(&self) -> Html {
        let node: &Node = &self.container.clone().into();
        Html::VRef(node.clone())
    }
}

fn calc_center(polygon: &geo::Polygon) -> geo::Point {
    let mut lon = 0.0;
    let mut lat = 0.0;
    for point in polygon.exterior().points() {
        lon += point.x();
        lat += point.y();
    }
    #[allow(clippy::cast_precision_loss)]
    let length = polygon.exterior().points().count() as f64;
    geo::Point(coord! {x: lon / length, y: lat / length}).tap(|p| debug!("Center: {:?}", p))
}

impl Component for MapComponent {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();

        let container: Element = document().create_element("div").unwrap();
        let container: HtmlElement = container.dyn_into().unwrap();
        container.set_class_name("map");
        let leaflet_map = Map::new_with_element(&container, &MapOptions::default());
        Self {
            map: leaflet_map,
            container,
            lat: props.lat,
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.map
                .set_view(&LatLng::new(self.lat.y(), self.lat.x()), 11.0);
            add_tile_layer(&self.map);
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, _msg: Self::Message) -> bool {
        false
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        let location_changed = if let Some(location) = &props.location {
            if props.location == old_props.location {
                false
            } else {
                self.lat = calc_center(&location.bounds);
                self.map
                    .set_view(&LatLng::new(self.lat.y(), self.lat.x()), 21.0);
                true
            }
        } else {
            false
        };

        let list_changed = if props.locations == old_props.locations {
            false
        } else {
            let options = leaflet::PolylineOptions::default();
            options.set_color("red".to_string());
            options.set_fill_color("red".to_string());
            options.set_weight(3.0);
            options.set_opacity(0.5);
            options.set_fill(true);

            for location in props.locations.iter() {
                debug!("Location: {:?}", location);

                let latlngs = location
                    .bounds
                    .exterior()
                    .coords()
                    .map(|latlng| LatLng::new(latlng.y, latlng.x))
                    .map(JsValue::from)
                    .collect();

                leaflet::Polyline::new_with_options(&latlngs, &options).add_to(&self.map);
            }

            true
        };

        location_changed || list_changed
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <div class="map-container component-container">
                {self.render_map()}
            </div>
        }
    }
}

fn add_tile_layer(map: &Map) {
    TileLayer::new("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png").add_to(map);
}
