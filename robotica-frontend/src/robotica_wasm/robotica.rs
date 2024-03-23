use super::macros::object_property_set;
use js_sys::Object;
use leaflet::{object_constructor, Control};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[derive(Clone, Debug)]
    #[wasm_bindgen(extends = Control, js_namespace = ["L", "Control"])]
    pub type Button;

    #[wasm_bindgen(js_namespace = ["L", "control"], js_name = "button")]
    fn constructor_button(options: &ButtonOptions) -> Button;

    #[wasm_bindgen(extends = Object , js_name = ButtonOptions)]
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[wasm_bindgen(extends = Control)]
    pub type ButtonOptions;
}

impl Button {
    /// Creates a new `Zoom` control.
    #[must_use]
    pub fn new(options: &ButtonOptions) -> Self {
        constructor_button(options)
    }
}

impl ButtonOptions {
    object_constructor!();
    object_property_set!(position, position, &str);
}

impl Default for ButtonOptions {
    fn default() -> Self {
        ButtonOptions::new()
    }
}
