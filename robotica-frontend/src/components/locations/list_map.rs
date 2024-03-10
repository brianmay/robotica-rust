use geo::coord;
use gloo_utils::document;
use leaflet::{LatLng, Map, MapOptions, TileLayer};
use robotica_common::robotica::locations::Location;
use std::sync::Arc;
use tap::Pipe;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{Element, HtmlElement, Node};
use yew::prelude::*;

use crate::robotica_wasm::draw_control;

pub enum Msg {}

pub struct ListMapComponent {
    map: Map,
    container: HtmlElement,
    draw_layer: leaflet::FeatureGroup,
    _create_handler: Closure<dyn FnMut(leaflet::Event)>,
    // _resize_handler: Closure<dyn FnMut(leaflet::Event)>,
}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub locations: Arc<Vec<Location>>,
    pub create_polygon: Callback<geo::Polygon>,
}

impl ListMapComponent {
    fn render_map(&self) -> Html {
        let node: &Node = &self.container.clone().into();
        Html::VRef(node.clone())
    }
}

impl Component for ListMapComponent {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        // let props = ctx.props();

        let container: Element = document().create_element("div").unwrap();
        let container: HtmlElement = container.dyn_into().unwrap();
        container.set_class_name("map");
        let leaflet_map = Map::new_with_element(&container, &MapOptions::default());

        let draw_layer = leaflet::FeatureGroup::new();
        draw_layer.add_to(&leaflet_map);

        let draw = {
            // let edit_options = draw_control::EditOptions::new();
            // edit_options.set_feature_group(draw_layer.clone());

            let draw_options = draw_control::DrawOptions::new();
            draw_options.set_polyline(false);
            draw_options.set_polygon(true);
            draw_options.set_rectangle(false);
            draw_options.set_circle(false);
            draw_options.set_marker(false);
            draw_options.set_circle_marker(false);

            let options = draw_control::DrawControlOptions::new();
            // options.set_edit(edit_options);
            options.set_draw(draw_options);

            draw_control::DrawControl::new(&options)
        };
        leaflet_map.add_control(&draw);

        let create_handler = create_handler(ctx);
        // Hack: Required to ensure the map fit_bounds works
        // let resize_handler = resize_handler(&leaflet_map, &draw_layer);

        leaflet_map.on("draw:created", create_handler.as_ref());
        // leaflet_map.on("resize", resize_handler.as_ref());

        add_tile_layer(&leaflet_map);

        // Hack: Trigger a resize event to force the map to render
        web_sys::window()
            .unwrap()
            .dispatch_event(&Event::new("resize").unwrap())
            .unwrap();

        Self {
            map: leaflet_map,
            container,
            draw_layer,
            _create_handler: create_handler,
            // _resize_handler: resize_handler,
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, _first_render: bool) {
        // if first_render {
        //     add_tile_layer(&self.map);
        // }
    }

    fn update(&mut self, _ctx: &Context<Self>, _msg: Self::Message) -> bool {
        false
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.locations == old_props.locations {
            false
        } else {
            let options = leaflet::PolylineOptions::default();
            options.set_weight(3.0);
            options.set_opacity(0.5);
            options.set_fill(true);

            self.draw_layer.clear_layers();
            for location in props.locations.iter() {
                options.set_color(location.color.clone());
                options.set_fill_color(location.color.clone());

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

            self.map.fit_bounds(self.draw_layer.get_bounds().as_ref());
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

fn create_handler(ctx: &Context<ListMapComponent>) -> Closure<dyn FnMut(leaflet::Event)> {
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

// fn resize_handler(
//     leaflet_map: &Map,
//     draw_layer: &leaflet::FeatureGroup,
// ) -> Closure<dyn FnMut(leaflet::Event)> {
//     let map = leaflet_map.clone();
//     let draw_layer = draw_layer.clone();
//     Closure::<dyn FnMut(_)>::new(move |_event: leaflet::Event| {
//         map.fit_bounds(draw_layer.get_bounds().as_ref());
//     })
// }

fn add_tile_layer(map: &Map) {
    TileLayer::new("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png").add_to(map);
}
