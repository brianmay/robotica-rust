use robotica_common::robotica::locations::Location;
use std::sync::Arc;
use yew::prelude::*;

pub enum Msg {}

pub struct List {
    locations: Arc<Vec<Location>>,
}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub locations: Arc<Vec<Location>>,
    pub select_location: Callback<Location>,
    pub cancel: Callback<()>,
}

impl List {
    fn button(ctx: &Context<Self>, location: Location) -> Html {
        let name = location.name.clone();
        let cb = ctx
            .props()
            .select_location
            .reform(move |_| location.clone());
        html! {
            <button onclick={cb}>{name}</button>
        }
    }
}

impl Component for List {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        List {
            locations: ctx.props().locations.clone(),
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let on_click = ctx.props().cancel.reform(|_| ());

        html! {
            <div class="control component-container">
                <h1>{"Choose a location"}</h1>
                <div>
                    {for self.locations.iter().map(|location| Self::button(ctx, location.clone()))}
                </div>
                <button onclick={on_click}>{"Cancel"}</button>
            </div>
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if old_props.locations == props.locations {
            false
        } else {
            self.locations = props.locations.clone();
            true
        }
    }
}
