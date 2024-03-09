use super::macros::create_object_with_properties;
use leaflet::Control;
use wasm_bindgen::prelude::*;
use web_sys::js_sys::Object;

#[wasm_bindgen]
extern "C" {
    #[derive(Debug, Clone)]
    #[wasm_bindgen(js_namespace = ["L", "Control"], js_name = Draw, extends = Control)]
    pub type DrawControl;

    #[wasm_bindgen(constructor, js_class = "L.Control.Draw")]
    pub fn new(options: &DrawControlOptions) -> DrawControl;
}

create_object_with_properties!(
    (DrawControlOptions, DrawControlOptions),
    // Options
    (edit, edit, EditOptions),
    (draw, draw, DrawOptions)
);

create_object_with_properties!(
    (EditOptions, EditOptions, Object),
    // Options
    (feature_group, featureGroup, leaflet::FeatureGroup)
);

create_object_with_properties!(
    (DrawOptions, DrawOptions, Object),
    // Options
    (polyline, polyline, bool),
    (polygon, polygon, bool),
    (rectangle, rectangle, bool),
    (circle, circle, bool),
    (marker, marker, bool),
    (circle_marker, circlemarker, bool)
);
