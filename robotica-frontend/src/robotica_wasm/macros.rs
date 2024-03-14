macro_rules! create_object_with_properties {
    (($t:ident, $t_js:ident), $(($rust:ident, $js:ident, $b:ty)),+) => {
        $crate::paste! {
            #[wasm_bindgen]
            extern "C" {
                #[wasm_bindgen (extends = Object , js_name = $t_js)]
                #[derive(Debug, Clone, PartialEq, Eq)]
                pub type $t;

                $(
                #[wasm_bindgen(method, getter, js_name = $js)]
                pub fn $rust(this: &$t) -> $b;
                )*

                $(
                #[wasm_bindgen(method, setter, js_name = $js)]
                pub fn [<set_ $rust>](this: &$t, val: $b);
                )*
            }
        }
        impl $t {
            #[allow(clippy::new_without_default)]
            #[must_use] pub fn new() -> Self {
                #[allow(unused_mut)]
                let mut r = JsCast::unchecked_into(Object::new());
                r
            }
        }
    };
    (($t:ident, $t_js:ident, $t_extends:ident), $(($rust:ident, $js:ident, $b:ty)),+) => {
        $crate::paste! {
            #[wasm_bindgen]
            extern "C" {
                #[wasm_bindgen(extends = $t_extends, js_name = $t_js)]
                #[derive(Debug, Clone, PartialEq, Eq)]
                pub type $t;

                $(
                #[wasm_bindgen(method, getter, js_name = $js)]
                pub fn $rust(this: &$t) -> $b;
                )*

                $(
                #[wasm_bindgen(method, setter, js_name = $js)]
                pub fn [<set_ $rust>](this: &$t, val: $b);
                )*
            }
        }
        impl $t {
            #[allow(clippy::new_without_default)]
            #[must_use] pub fn new() -> Self {
                #[allow(unused_mut)]
                let mut r = JsCast::unchecked_into(Object::new());
                r
            }
        }
    };
}

pub(crate) use create_object_with_properties;
