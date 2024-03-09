use wasm_bindgen::JsCast;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct Props {
    pub label: String,
    pub value: String,
    pub on_change: Callback<String>,
}

#[function_component(TextInput)]
pub fn text_input(props: &Props) -> Html {
    let on_change = props.on_change.reform(|e: Event| {
        e.prevent_default();
        e.target()
            .unwrap()
            .unchecked_into::<web_sys::HtmlInputElement>()
            .value()
    });
    // let xx = props.on_change.clone();
    // let on_change = Callback::from(move |e: Event| {
    //     e.prevent_default();
    //     e.target()
    //         .unwrap()
    //         .unchecked_into::<web_sys::HtmlInputElement>()
    //         .value()
    //         .pipe(|e| xx.emit(e));
    // });
    html! {
        <input
            type="text"
            value={props.value.clone()}
            onchange={on_change}
        />
    }
}
