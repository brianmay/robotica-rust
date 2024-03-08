use std::sync::Arc;

use super::locations::LocationWrapper;
use tracing::debug;
use yew::prelude::*;

pub enum Msg {
    LocationChosen(LocationWrapper),
}

pub struct Control {
    locations: Arc<Vec<LocationWrapper>>,
}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub locations: Arc<Vec<LocationWrapper>>,
    pub select_location: Callback<LocationWrapper>,
}

impl Control {
    fn button(ctx: &Context<Self>, location: LocationWrapper) -> Html {
        let name = location.name.clone();
        let cb = ctx
            .link()
            .callback(move |_| Msg::LocationChosen(location.clone()));
        html! {
            <button onclick={cb}>{name}</button>
        }
    }
}

impl Component for Control {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        Control {
            locations: ctx.props().locations.clone(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LocationChosen(location) => {
                debug!("Update: {:?}", location.name);
                ctx.props().select_location.emit(location);
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div class="control component-container">
                <h1>{"Choose a location"}</h1>
                <div>
                    {for self.locations.iter().map(|location| Self::button(ctx, location.clone()))}
                </div>
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
