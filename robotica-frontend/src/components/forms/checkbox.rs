use wasm_bindgen::JsCast;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct Props {
    pub id: String,
    pub label: String,
    pub value: bool,
    pub on_change: Callback<String>,
}

#[function_component(Checkbox)]
pub fn checkbox(props: &Props) -> Html {
    let on_change = props.on_change.reform(|e: Event| {
        e.prevent_default();
        e.target()
            .unwrap()
            .unchecked_into::<web_sys::HtmlInputElement>()
            .value()
    });
    let id = props.id.clone();
    let value = if props.value { "true" } else { "false" };
    html! {
        <>
            <input type="checkbox" id={id.clone()} name={id.clone()} value={value} onchange={on_change} checked={props.value} />
            <label for={id}>{props.label.clone()}</label>
            <br/>
        </>
    }
}
