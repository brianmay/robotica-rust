use super::{control::Control, ActionLocation};
use crate::components::{
    forms::{checkbox::Checkbox, text_input::TextInput},
    locations::map::{MapComponent, ParamObject},
};
use gloo_net::http::Request;
use reqwasm::{http::Response, Error};
use robotica_common::robotica::{
    http_api::ApiResponse,
    locations::{CreateLocation, Location},
};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use tap::Pipe;
use tracing::{debug, error};
use yew::{platform::spawn_local, prelude::*};

pub enum Msg {
    LoadFailed(String),
    SelectLocation(Location),
    Locations(Arc<Vec<Location>>),
    UpdateName(String),
    UpdateColor(String),
    UpdateAnnounceOnEnter(bool),
    UpdateAnnounceOnExit(bool),
    Save,
    SaveSuccess(Location),
    CreateSuccess(Location),
    SaveFailed(String),
    Cancel,
    DeleteSuccess(Location),
    CreateLocation(CreateLocation),
    UpdateLocation(ActionLocation),
    DeleteLocation(ActionLocation),
}

pub enum LoadingStatus {
    Loading,
    Loaded(Arc<Vec<Location>>),
    Error(String),
}

pub enum LocationStatus {
    Unchanged,
    Changed,
    Saving,
    Error(String),
}

impl LocationStatus {
    pub fn error(message: impl Into<String>) -> Self {
        LocationStatus::Error(message.into())
    }

    pub const fn can_save(&self) -> bool {
        matches!(
            self,
            LocationStatus::Unchanged | LocationStatus::Changed | LocationStatus::Error(_)
        )
    }
}

pub struct LocationsView {
    location: Option<ActionLocation>,
    status: LocationStatus,
    loading_status: LoadingStatus,
}

async fn process_response<T: DeserializeOwned + std::fmt::Debug>(
    response: Result<Response, Error>,
) -> Result<T, String> {
    match response {
        Ok(response) => {
            let api_response: Option<ApiResponse<T>> = response
                .json()
                .await
                .map_err(|err| error!("Error parsing server response: {err:?}"))
                .ok();
            debug!("api_response: {:?}", api_response);

            match (response.ok(), api_response) {
                (true, Some(ApiResponse::Success(response))) => Ok(response.data),
                (true, None) => Err("Invalid response".to_string()),
                (_, Some(ApiResponse::Error(err))) => err.message.pipe(Err),
                (false, _) => response.status_text().pipe(Err),
            }
        }
        Err(err) => format!("http error: {err:?}").pipe(Err),
    }
}

impl Component for LocationsView {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        load_list(ctx);

        Self {
            location: None,
            status: LocationStatus::Unchanged,
            loading_status: LoadingStatus::Loading,
        }
    }

    #[allow(clippy::cognitive_complexity)]
    #[allow(clippy::too_many_lines)]
    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFailed(err) => {
                self.loading_status = LoadingStatus::Error(err);
                true
            }
            Msg::SelectLocation(location) => {
                self.location = Some(ActionLocation::Update(location));
                self.status = LocationStatus::Unchanged;
                true
            }
            Msg::Locations(locations) => {
                self.loading_status = LoadingStatus::Loaded(locations);
                true
            }
            Msg::UpdateName(name) => {
                debug!("Updating name: {}", name);
                if let Some(location) = &mut self.location {
                    location.set_name(name);
                    self.status = LocationStatus::Changed;
                }
                true
            }
            Msg::UpdateColor(color) => {
                debug!("Updating color: {}", color);
                if let Some(location) = &mut self.location {
                    location.set_color(color);
                    self.status = LocationStatus::Changed;
                }
                true
            }
            Msg::UpdateAnnounceOnEnter(announce_on_enter) => {
                debug!("Updating announce_on_enter: {}", announce_on_enter);
                if let Some(location) = &mut self.location {
                    location.set_announce_on_enter(announce_on_enter);
                    self.status = LocationStatus::Changed;
                }
                true
            }
            Msg::UpdateAnnounceOnExit(announce_on_exit) => {
                debug!("Updating announce_on_exit: {}", announce_on_exit);
                if let Some(location) = &mut self.location {
                    location.set_announce_on_exit(announce_on_exit);
                    self.status = LocationStatus::Changed;
                }
                true
            }
            Msg::CreateLocation(location) => {
                debug!("Creating location: {:?}", location);
                let action_location = ActionLocation::Create(location);
                self.status = LocationStatus::Saving;
                save_location(&action_location, ctx);
                true
            }
            Msg::UpdateLocation(location) => {
                debug!("Updating location: {:?}", location);
                self.status = LocationStatus::Saving;
                save_location(&location, ctx);
                true
            }
            Msg::DeleteLocation(location) => {
                debug!("Deleting location: {:?}", location);
                self.status = LocationStatus::Saving;
                match &location {
                    ActionLocation::Create(_) => self.location = None,
                    ActionLocation::Update(location) => delete_location(location, ctx),
                }
                true
            }
            Msg::Save => match &mut self.location {
                Some(location_state) if self.status.can_save() => {
                    self.status = LocationStatus::Saving;
                    save_location(location_state, ctx);
                    true
                }

                Some(_location_state) => {
                    debug!("Status wrong, not saving");
                    false
                }

                None => {
                    debug!("No location selected, not saving");
                    false
                }
            },
            Msg::CreateSuccess(location) => {
                self.location = location.pipe(ActionLocation::Update).pipe(Some);
                self.status = LocationStatus::Unchanged;
                load_list(ctx);
                true
            }
            Msg::SaveSuccess(_location) => {
                self.location = None;
                self.status = LocationStatus::Unchanged;
                load_list(ctx);
                true
            }
            Msg::SaveFailed(error) => {
                if let Some(_location) = &mut self.location {
                    self.status = LocationStatus::error(error);
                    true
                } else {
                    false
                }
            }
            Msg::Cancel => {
                debug!("Cancelling");
                self.location = None;
                self.status = LocationStatus::Unchanged;
                true
            }
            Msg::DeleteSuccess(_location) => {
                self.location = None;
                self.status = LocationStatus::Unchanged;
                load_list(ctx);
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let locations = match &self.loading_status {
            LoadingStatus::Loaded(locations) => locations.clone(),
            LoadingStatus::Error(_) | LoadingStatus::Loading => Arc::new(Vec::new()),
        };

        let status_msg = match &self.status {
            LocationStatus::Unchanged => "Unchanged".to_string(),
            LocationStatus::Changed => "Changed".to_string(),
            LocationStatus::Saving => "Saving".to_string(),
            LocationStatus::Error(err) => format!("Error {err}"),
        };

        let controls = if let Some(location) = &self.location {
            let save = ctx.link().callback(|e: MouseEvent| {
                e.prevent_default();
                Msg::Save
            });

            let cancel = ctx.link().callback(|e: MouseEvent| {
                e.prevent_default();
                Msg::Cancel
            });

            let update_name = ctx.link().callback(Msg::UpdateName);
            let update_color = ctx.link().callback(Msg::UpdateColor);
            let update_announce_on_enter = ctx.link().callback(|x| {
                debug! {x};
                Msg::UpdateAnnounceOnEnter(x != "true")
            });
            let update_announce_on_exit = ctx
                .link()
                .callback(|x| Msg::UpdateAnnounceOnExit(x != "true"));

            let disable_save = !self.status.can_save();

            let name = location.name();

            html! {
                <>
                    <h1>{name.clone()}</h1>
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
        } else {
            let select_location = ctx.link().callback(Msg::SelectLocation);

            let msg = match &self.loading_status {
                LoadingStatus::Loading => "Loading".to_string(),
                LoadingStatus::Loaded(_) => "Loaded".to_string(),
                LoadingStatus::Error(err) => format!("Error {err}"),
            };

            html! {
                <>
                    <h1>{"Locations"}</h1>
                    <Control select_location={select_location} locations={locations.clone()}/>
                    <p>{msg}</p>
                    <p>{status_msg}</p>
                </>
            }
        };

        let object = if let Some(location) = &self.location {
            ParamObject::Item(location.clone())
        } else {
            ParamObject::List(locations)
        };

        {
            let create_location = ctx.link().callback(Msg::CreateLocation);
            let update_location = ctx.link().callback(Msg::UpdateLocation);
            let delete_location = ctx.link().callback(Msg::DeleteLocation);

            html! {
                <>
                    <MapComponent object={object} create_location={create_location} update_location={update_location} delete_location={delete_location} />
                    {controls}
                </>
            }
        }
    }
}

fn load_list(ctx: &Context<LocationsView>) {
    let link = ctx.link().clone();
    spawn_local(async move {
        let locations = Request::get("/api/locations")
            .send()
            .await
            .pipe(process_response::<Vec<Location>>)
            .await;

        let result = match locations {
            Ok(locations) => Msg::Locations(Arc::new(locations)),
            Err(err) => Msg::LoadFailed(err),
        };

        link.send_message(result);
    });
}

fn save_location(location: &ActionLocation, ctx: &Context<LocationsView>) {
    debug!("Saving location: {:?}", location);
    let location = location.clone();
    let link = ctx.link().clone();
    spawn_local(async move {
        debug!("Sending save request");

        let response = match location {
            ActionLocation::Create(location) => Request::post("/api/locations/create")
                .json(&location)
                .unwrap()
                .send()
                .await
                .pipe(process_response::<Location>)
                .await
                .map(Msg::CreateSuccess),

            ActionLocation::Update(location) => Request::put("/api/locations")
                .json(&location)
                .unwrap()
                .send()
                .await
                .pipe(process_response::<Location>)
                .await
                .map(Msg::SaveSuccess),
        };

        let result = match response {
            Ok(response) => {
                debug!("Location saved");
                response
            }
            Err(err) => {
                debug!("Failed to save location: {err}");
                Msg::SaveFailed(format!("Failed to save location: {err}"))
            }
        };

        link.send_message(result);
    });
}

fn delete_location(location: &Location, ctx: &Context<LocationsView>) {
    debug!("Deleting location: {:?}", location);
    let id = location.id;
    let location = location.clone();

    let link = ctx.link().clone();
    spawn_local(async move {
        debug!("Sending delete request");
        let response = Request::delete(&format!("/api/locations/{id}"))
            .send()
            .await
            .pipe(process_response::<()>)
            .await;

        let result = match response {
            Ok(()) => {
                debug!("Location deleted");
                Msg::DeleteSuccess(location.clone())
            }
            Err(err) => {
                debug!("Failed to delete location: {err}");
                Msg::SaveFailed(format!("Failed to delete location: {err}"))
            }
        };

        link.send_message(result);
    });
}
