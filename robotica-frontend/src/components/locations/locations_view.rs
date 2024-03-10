use super::{
    control::Control, item_map::ItemMapComponent, list_map::ListMapComponent, ActionLocation,
};
use crate::components::forms::text_input::TextInput;
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
    Save,
    SaveSuccess(Location),
    SaveFailed(String),
    DeleteSuccess,
    CreatePolygon(geo::Polygon),
    UpdatePolygon(geo::Polygon),
    DeletePolygon,
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
    Saved,
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

pub struct LocationState {
    pub location: ActionLocation,
    pub status: LocationStatus,
}

pub struct LocationsView {
    location_state: Option<LocationState>,
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
            location_state: None,
            loading_status: LoadingStatus::Loading,
        }
    }

    #[allow(clippy::cognitive_complexity)]
    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFailed(err) => {
                self.loading_status = LoadingStatus::Error(err);
                true
            }
            Msg::SelectLocation(location) => {
                self.location_state = Some(LocationState {
                    location: ActionLocation::Update(location),
                    status: LocationStatus::Unchanged,
                });
                true
            }
            Msg::Locations(locations) => {
                self.loading_status = LoadingStatus::Loaded(locations);
                true
            }
            Msg::UpdateName(name) => {
                debug!("Updating name: {}", name);
                if let Some(location_state) = &mut self.location_state {
                    location_state.location.set_name(name);
                    location_state.status = LocationStatus::Changed;
                }
                true
            }
            Msg::CreatePolygon(polygon) => {
                debug!("Creating polygon: {:?}", polygon);
                if let Some(location_state) = &mut self.location_state {
                    location_state.location = CreateLocation {
                        bounds: polygon,
                        name: location_state.location.name(),
                    }
                    .pipe(ActionLocation::Create);
                    location_state.status = LocationStatus::Changed;
                }
                true
            }
            Msg::UpdatePolygon(polygon) => {
                debug!("Updating polygon: {:?}", polygon);
                if let Some(location_state) = &mut self.location_state {
                    location_state.location.set_bounds(polygon);
                    location_state.status = LocationStatus::Changed;
                }
                true
            }
            Msg::DeletePolygon => {
                debug!("Deleting polygon");
                if let Some(location_state) = &mut self.location_state {
                    location_state.status = LocationStatus::Changed;
                    match &location_state.location {
                        ActionLocation::Create(_) => self.location_state = None,
                        ActionLocation::Update(location) => {
                            delete_location(location, ctx);
                        }
                    }
                }
                true
            }
            Msg::Save => match &mut self.location_state {
                Some(location_state) if location_state.status.can_save() => {
                    location_state.status = LocationStatus::Saving;
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
            Msg::SaveSuccess(location) => {
                if let Some(location_state) = &mut self.location_state {
                    location_state.location = ActionLocation::Update(location);
                    location_state.status = LocationStatus::Saved;
                    load_list(ctx);
                    true
                } else {
                    false
                }
            }
            Msg::DeleteSuccess => {
                self.location_state = None;
                true
            }
            Msg::SaveFailed(error) => {
                if let Some(location_state) = &mut self.location_state {
                    location_state.status = LocationStatus::error(error);
                    true
                } else {
                    false
                }
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let select_location = ctx.link().callback(Msg::SelectLocation);

        let save = ctx.link().callback(|e: MouseEvent| {
            e.prevent_default();
            Msg::Save
        });

        let locations = match &self.loading_status {
            LoadingStatus::Loaded(locations) => locations.clone(),
            LoadingStatus::Error(_) | LoadingStatus::Loading => Arc::new(Vec::new()),
        };

        if let Some(location_state) = &self.location_state {
            let update_name = ctx.link().callback(Msg::UpdateName);
            let create_polygon = ctx.link().callback(Msg::CreatePolygon);
            let update_polygon = ctx.link().callback(Msg::UpdatePolygon);
            let delete_polygon = ctx.link().callback(|()| Msg::DeletePolygon);

            let disable_save = !location_state.status.can_save();

            let msg = match &location_state.status {
                LocationStatus::Unchanged => "Unchanged".to_string(),
                LocationStatus::Changed => "Changed".to_string(),
                LocationStatus::Saving => "Saving".to_string(),
                LocationStatus::Saved => "Saved".to_string(),
                LocationStatus::Error(err) => format!("Error {err}"),
            };

            let name = location_state.location.name();

            html! {
                <>
                    <h1>{name.clone()}</h1>
                    <ItemMapComponent location={location_state.location.clone()} create_polygon={create_polygon} update_polygon={update_polygon} delete_polygon={delete_polygon} />
                    <Control select_location={select_location} locations={locations}/>
                    <form>
                        <TextInput label="Name" value={name} on_change={update_name} />
                        <button onclick={save} disabled={disable_save} >
                            {"Save"}
                        </button>
                        <p>{msg}</p>
                    </form>
                </>
            }
        } else {
            let msg = match &self.loading_status {
                LoadingStatus::Loading => "Loading".to_string(),
                LoadingStatus::Loaded(_) => "Loaded".to_string(),
                LoadingStatus::Error(err) => format!("Error {err}"),
            };

            html! {
                <>
                    <h1>{"Locations"}</h1>
                    <ListMapComponent locations={locations.clone()}  />
                    <Control select_location={select_location} locations={locations}/>
                    <p>{msg}</p>
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

fn save_location(location_state: &LocationState, ctx: &Context<LocationsView>) {
    debug!("Saving location: {:?}", location_state.location);
    let location = location_state.location.clone();
    let link = ctx.link().clone();
    spawn_local(async move {
        debug!("Sending request");

        let response = match location {
            ActionLocation::Create(location) => {
                Request::post("/api/locations/create")
                    .json(&location)
                    .unwrap()
                    .send()
                    .await
                    .pipe(process_response::<Location>)
                    .await
            }

            ActionLocation::Update(location) => {
                Request::put("/api/locations")
                    .json(&location)
                    .unwrap()
                    .send()
                    .await
                    .pipe(process_response::<Location>)
                    .await
            }
        };

        let result = match response {
            Ok(response) => {
                debug!("Location saved");
                Msg::SaveSuccess(response)
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
    let id = location.id;
    let link = ctx.link().clone();
    spawn_local(async move {
        let response = Request::delete(&format!("/api/locations/{id}"))
            .send()
            .await
            .pipe(process_response::<()>)
            .await;

        let result = match response {
            Ok(()) => {
                debug!("Location deleted");
                Msg::DeleteSuccess
            }
            Err(err) => {
                debug!("Failed to delete location: {err}");
                Msg::SaveFailed(format!("Failed to delete location: {err}"))
            }
        };

        link.send_message(result);
    });
}
