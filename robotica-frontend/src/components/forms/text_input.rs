use wasm_bindgen::JsCast;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct Props {
    pub id: String,
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
    let id = props.id.clone();
    html! {
        <>
            <label for={id.clone()}>{props.label.clone()}</label>
            <input
                type="text"
                id={id}
                value={props.value.clone()}
                onchange={on_change}
                placeholder={props.label.clone()}
            />
            <br/>
        </>
    }
}
