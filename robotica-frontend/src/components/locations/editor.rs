use super::{locations_view::LocationStatus, ActionLocation};
use crate::components::forms::{checkbox::Checkbox, text_input::TextInput};
use tracing::debug;
use yew::prelude::*;

pub enum Msg {
    Name(String),
    Color(String),
    AnnounceOnEnter(bool),
    AnnounceOnExit(bool),
}

pub enum UpdateLocation {
    Name(String),
    Color(String),
    AnnounceOnEnter(bool),
    AnnounceOnExit(bool),
}

impl UpdateLocation {
    pub fn apply_to_location(&self, location: &mut ActionLocation) {
        match self {
            UpdateLocation::Name(name) => location.set_name(name.clone()),
            UpdateLocation::Color(color) => location.set_color(color.clone()),
            UpdateLocation::AnnounceOnEnter(announce_on_enter) => {
                location.set_announce_on_enter(*announce_on_enter);
            }
            UpdateLocation::AnnounceOnExit(announce_on_exit) => {
                location.set_announce_on_exit(*announce_on_exit);
            }
        }
    }
}

pub struct EditorView {}

#[derive(PartialEq, Clone, Properties)]
pub struct Props {
    pub location: ActionLocation,
    pub status: LocationStatus,
    pub update_location: Callback<UpdateLocation>,
    pub on_save: Callback<()>,
    pub on_cancel: Callback<()>,
}

impl Component for EditorView {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    #[allow(clippy::cognitive_complexity)]
    #[allow(clippy::too_many_lines)]
    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Name(name) => {
                debug!("Updating name: {}", name);
                let props = ctx.props();
                let update = UpdateLocation::Name(name);
                props.update_location.emit(update);
                false
            }
            Msg::Color(color) => {
                debug!("Updating color: {}", color);
                let props = ctx.props();
                let update = UpdateLocation::Color(color);
                props.update_location.emit(update);
                false
            }
            Msg::AnnounceOnEnter(announce_on_enter) => {
                debug!("Updating announce_on_enter: {}", announce_on_enter);
                let props = ctx.props();
                let update = UpdateLocation::AnnounceOnEnter(announce_on_enter);
                props.update_location.emit(update);
                false
            }
            Msg::AnnounceOnExit(announce_on_exit) => {
                debug!("Updating announce_on_exit: {}", announce_on_exit);
                let props = ctx.props();
                let update = UpdateLocation::AnnounceOnExit(announce_on_exit);
                props.update_location.emit(update);
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let location = &props.location;
        let status = &props.status;

        let status_msg = match status {
            LocationStatus::Unchanged => "Unchanged".to_string(),
            LocationStatus::Changed => "Changed".to_string(),
            LocationStatus::Saving => "Saving".to_string(),
            LocationStatus::Error(err) => format!("Error {err}"),
        };

        let save = props.on_save.reform(|_| ());
        let cancel = props.on_cancel.reform(|_| ());

        let update_name = ctx.link().callback(Msg::Name);
        let update_color = ctx.link().callback(Msg::Color);
        let update_announce_on_enter = ctx.link().callback(|x| {
            debug! {x};
            Msg::AnnounceOnEnter(x != "true")
        });
        let update_announce_on_exit = ctx.link().callback(|x| Msg::AnnounceOnExit(x != "true"));

        let disable_save = !status.can_save();

        let name = location.name();

        html! {
            <>
                <h1>{&name}</h1>
                <form>
                    <TextInput id="name" label="Name" value={name} on_change={update_name} />
                    <TextInput id="color" label="Color" value={location.color()} on_change={update_color} />
                    <Checkbox id="announce_on_enter" label="Announce on enter" value={location.announce_on_enter()} on_change={update_announce_on_enter} />
                    <Checkbox id="announce_on_exit" label="Announce on exit" value={location.announce_on_exit()} on_change={update_announce_on_exit} />

                    <button onclick={save} disabled={disable_save} >
                        {"Save"}
                    </button>
                    <button onclick={cancel} >
                        {"Cancel"}
                    </button>
                    <p>{status_msg}</p>
                </form>
            </>
        }
    }
}
