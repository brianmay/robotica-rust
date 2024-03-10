use gloo_utils::document;
use leaflet::{LatLng, Map, MapOptions, TileLayer};
use robotica_common::robotica::locations::Location;
use std::sync::Arc;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, HtmlElement, Node};
use yew::prelude::*;

pub enum Msg {}

pub struct ListMapComponent {
    map: Map,
    container: HtmlElement,
    draw_layer: leaflet::FeatureGroup,
}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub locations: Arc<Vec<Location>>,
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

    fn create(_ctx: &Context<Self>) -> Self {
        // let props = ctx.props();

        let container: Element = document().create_element("div").unwrap();
        let container: HtmlElement = container.dyn_into().unwrap();
        container.set_class_name("map");
        let leaflet_map = Map::new_with_element(&container, &MapOptions::default());

        let draw_layer = leaflet::FeatureGroup::new();
        draw_layer.add_to(&leaflet_map);

        add_tile_layer(&leaflet_map);

        Self {
            map: leaflet_map,
            container,
            draw_layer,
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

fn add_tile_layer(map: &Map) {
    TileLayer::new("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png").add_to(map);
}
