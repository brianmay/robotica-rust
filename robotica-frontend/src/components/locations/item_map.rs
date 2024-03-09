use crate::robotica_wasm::draw_control;
use geo::coord;
use gloo_utils::document;
use leaflet::{LatLng, Map, MapOptions, TileLayer};
use robotica_common::robotica::locations::Location;
use tap::Tap;
use tracing::debug;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{Element, HtmlElement, Node};
use yew::prelude::*;

pub enum Msg {}

pub struct ItemMapComponent {
    map: Map,
    container: HtmlElement,
    draw_layer: leaflet::FeatureGroup,
    _create_handler: Closure<dyn FnMut(leaflet::Event)>,
}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub location: Location,
}

impl ItemMapComponent {
    fn render_map(&self) -> Html {
        let node: &Node = &self.container.clone().into();
        Html::VRef(node.clone())
    }

    fn draw_location(&self, location: &Location) {
        let options = leaflet::PolylineOptions::default();
        options.set_color("red".to_string());
        options.set_fill_color("red".to_string());
        options.set_weight(3.0);
        options.set_opacity(0.5);
        options.set_fill(true);

        self.draw_layer.clear_layers();
        let latlngs = location
            .bounds
            .exterior()
            .coords()
            .map(|latlng| LatLng::new(latlng.y, latlng.x))
            .map(JsValue::from)
            .collect();

        leaflet::Polygon::new_with_options(&latlngs, &options)
            .unchecked_into::<leaflet::Layer>()
            .add_to_layer_group(&self.draw_layer);

        let lat = calc_center(&location.bounds);
        self.map.set_view(&LatLng::new(lat.y(), lat.x()), 21.0);
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

impl Component for ItemMapComponent {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();

        let container: Element = document().create_element("div").unwrap();
        let container: HtmlElement = container.dyn_into().unwrap();
        container.set_class_name("map");
        let leaflet_map = Map::new_with_element(&container, &MapOptions::default());

        let draw_layer = leaflet::FeatureGroup::new();
        draw_layer.add_to(&leaflet_map);

        let draw = {
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
            options.set_edit(edit_options);
            options.set_draw(draw_options);

            draw_control::DrawControl::new(&options)
        };
        leaflet_map.add_control(&draw);

        let draw_layer_clone = draw_layer.clone();
        let create_handler = Closure::<dyn FnMut(_)>::new(move |event: leaflet::Event| {
            debug!("Event: {:?}", event);
            event
                .layer()
                .unchecked_into::<leaflet::Layer>()
                .unchecked_into::<leaflet::Polyline>()
                .get_lat_lngs()
                .iter()
                .for_each(|latlng| {
                    debug!("Latlng: {:?}", latlng);
                });
            draw_layer_clone.add_layer(&event.layer().unchecked_into::<leaflet::Layer>());
        });

        leaflet_map.on("draw:created", create_handler.as_ref());

        // Trigger a resize event to force the map to render
        web_sys::window()
            .unwrap()
            .dispatch_event(&Event::new("resize").unwrap())
            .unwrap();

        Self {
            map: leaflet_map,
            container,
            draw_layer,
            _create_handler: create_handler,
        }
        .tap(|s| s.draw_location(&props.location))
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            // self.map.set_view(&LatLng::new(0.0, 0.0), 11.0);
            add_tile_layer(&self.map);
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, _msg: Self::Message) -> bool {
        false
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.location == old_props.location {
            false
        } else {
            self.draw_location(&props.location);

            let lat = calc_center(&props.location.bounds);
            self.map.set_view(&LatLng::new(lat.y(), lat.x()), 21.0);

            true
        }
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
