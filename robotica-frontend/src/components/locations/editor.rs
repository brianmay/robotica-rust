use super::{zones::ZoneStatus, ActionZone};
use crate::components::forms::{checkbox::Checkbox, text_input::TextInput};
use tracing::debug;
use yew::prelude::*;

pub enum Msg {
    Name(String),
    Color(String),
    AnnounceOnEnter(bool),
    AnnounceOnExit(bool),
}

pub enum UpdateZone {
    Name(String),
    Color(String),
    AnnounceOnEnter(bool),
    AnnounceOnExit(bool),
}

impl UpdateZone {
    pub fn apply_to_zone(&self, zone: &mut ActionZone) {
        match self {
            UpdateZone::Name(name) => zone.set_name(name.clone()),
            UpdateZone::Color(color) => zone.set_color(color.clone()),
            UpdateZone::AnnounceOnEnter(announce_on_enter) => {
                zone.set_announce_on_enter(*announce_on_enter);
            }
            UpdateZone::AnnounceOnExit(announce_on_exit) => {
                zone.set_announce_on_exit(*announce_on_exit);
            }
        }
    }
}

pub struct EditorView {}

#[derive(PartialEq, Clone, Properties)]
pub struct Props {
    pub zone: ActionZone,
    pub status: ZoneStatus,
    pub update_zone: Callback<UpdateZone>,
    pub on_save: Callback<()>,
    pub on_delete: Callback<()>,
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
                let update = UpdateZone::Name(name);
                props.update_zone.emit(update);
                false
            }
            Msg::Color(color) => {
                debug!("Updating color: {}", color);
                let props = ctx.props();
                let update = UpdateZone::Color(color);
                props.update_zone.emit(update);
                false
            }
            Msg::AnnounceOnEnter(announce_on_enter) => {
                debug!("Updating announce_on_enter: {}", announce_on_enter);
                let props = ctx.props();
                let update = UpdateZone::AnnounceOnEnter(announce_on_enter);
                props.update_zone.emit(update);
                false
            }
            Msg::AnnounceOnExit(announce_on_exit) => {
                debug!("Updating announce_on_exit: {}", announce_on_exit);
                let props = ctx.props();
                let update = UpdateZone::AnnounceOnExit(announce_on_exit);
                props.update_zone.emit(update);
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let zone = &props.zone;
        let status = &props.status;

        let status_msg = match status {
            ZoneStatus::Unchanged => "Unchanged".to_string(),
            ZoneStatus::Changed => "Changed".to_string(),
            ZoneStatus::Saving => "Saving".to_string(),
            ZoneStatus::Error(err) => format!("Error {err}"),
        };

        let save = props.on_save.reform(|_| ());
        let cancel = props.on_cancel.reform(|_| ());
        let delete = props.on_delete.reform(|_| ());

        let update_name = ctx.link().callback(Msg::Name);
        let update_color = ctx.link().callback(Msg::Color);
        let update_announce_on_enter = ctx.link().callback(|x| {
            debug! {x};
            Msg::AnnounceOnEnter(x != "true")
        });
        let update_announce_on_exit = ctx.link().callback(|x| Msg::AnnounceOnExit(x != "true"));

        let disable_save = !status.can_save();

        let name = zone.name();

        html! {
            <>
                <h1>{&name}</h1>
                <form>
                    <TextInput id="name" label="Name" value={name} on_change={update_name} />
                    <TextInput id="color" label="Color" value={zone.color()} on_change={update_color} />
                    <Checkbox id="announce_on_enter" label="Announce on enter" value={zone.announce_on_enter()} on_change={update_announce_on_enter} />
                    <Checkbox id="announce_on_exit" label="Announce on exit" value={zone.announce_on_exit()} on_change={update_announce_on_exit} />

                    <button type="button" onclick={save} disabled={disable_save} >
                        {"Save"}
                    </button>
                    <button type="button" onclick={cancel} >
                        {"Cancel"}
                    </button>
                    <button type="button" onclick={delete} disabled={disable_save} class="delete">
                        {"Delete Zone"}
                    </button>
                    <p>{status_msg}</p>
                </form>
            </>
        }
    }
}
