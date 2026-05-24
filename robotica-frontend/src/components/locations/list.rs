use robotica_common::robotica::zones::Zone;
use std::sync::Arc;
use yew::prelude::*;

pub enum Msg {}

pub struct List {
    locations: Arc<Vec<Zone>>,
}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub locations: Arc<Vec<Zone>>,
    pub select_location: Callback<Zone>,
    pub cancel: Callback<()>,
}

impl List {
    fn button(ctx: &Context<Self>, zone: Zone) -> Html {
        let name = zone.name.clone();
        let cb = ctx
            .props()
            .select_location
            .reform(move |_| zone.clone());
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
                    {for self.locations.iter().map(|zone| Self::button(ctx, zone.clone()))}
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
