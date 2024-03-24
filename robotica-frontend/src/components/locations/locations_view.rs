use super::ActionLocation;
use crate::components::locations::map::{MapComponent, ParamObject};
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
    Locations(Arc<Vec<Location>>),
    SaveLocation(ActionLocation),
    SaveSuccess(Location),
    CreateSuccess(Location),
    SaveFailed(ActionLocation, String),
    DeleteSuccess(Location),
    DeleteFailed(Location, String),
    CreateLocation(CreateLocation),
    UpdateLocation(ActionLocation),
    DeleteLocation(ActionLocation),
    RequestItem(Location),
    RequestList,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadingStatus {
    Loading,
    Loaded,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    list: Arc<Vec<Location>>,
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
            // debug!("api_response: {:?}", api_response);

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
            list: Arc::new(Vec::new()),
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
            Msg::Locations(locations) => {
                self.loading_status = LoadingStatus::Loaded;
                self.list = locations;
                true
            }
            Msg::CreateLocation(location) => {
                debug!("Creating location: {:?}", location);
                let action_location = ActionLocation::Create(location);
                self.status = LocationStatus::Saving;
                save_location(action_location, ctx);
                true
            }
            Msg::UpdateLocation(location) => {
                debug!("Updating location: {:?}", location);
                self.location = Some(location);
                self.status = LocationStatus::Changed;
                true
            }
            Msg::DeleteLocation(location) => {
                debug!("Deleting location: {:?}", location);
                self.status = LocationStatus::Saving;
                match location {
                    ActionLocation::Create(_) => self.location = None,
                    ActionLocation::Update(location) => delete_location(location, ctx),
                }
                true
            }
            Msg::SaveLocation(location) => {
                if self.status.can_save() {
                    self.status = LocationStatus::Saving;
                    save_location(location, ctx);
                    true
                } else {
                    debug!("Status wrong, not saving");
                    false
                }
            }
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
            Msg::SaveFailed(location, error) => {
                self.status = LocationStatus::error(error);
                self.location = Some(location);
                true
            }
            Msg::DeleteSuccess(_location) => {
                self.location = None;
                self.status = LocationStatus::Unchanged;
                self.loading_status = LoadingStatus::Loading;
                load_list(ctx);
                true
            }
            Msg::DeleteFailed(_location, error) => {
                self.status = LocationStatus::error(error);
                true
            }
            Msg::RequestItem(location) => {
                self.location = Some(ActionLocation::Update(location));
                self.status = LocationStatus::Unchanged;
                true
            }
            Msg::RequestList => {
                self.location = None;
                self.status = LocationStatus::Unchanged;
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let locations = self.list.clone();

        let object = if let Some(location) = &self.location {
            ParamObject::Item(location.clone())
        } else {
            ParamObject::List(locations)
        };

        let create_location = ctx.link().callback(Msg::CreateLocation);
        let update_location = ctx.link().callback(Msg::UpdateLocation);
        let delete_location = ctx.link().callback(Msg::DeleteLocation);
        let save_location = ctx.link().callback(Msg::SaveLocation);
        let request_item = ctx.link().callback(Msg::RequestItem);
        let request_list = ctx.link().callback(|()| Msg::RequestList);

        html! {
            <MapComponent
                object={object}
                status={self.status.clone()}
                loading_status={self.loading_status.clone()}
                create_location={create_location}
                update_location={update_location}
                delete_location={delete_location}
                save_location={save_location}
                request_item={request_item}
                request_list={request_list}
                />
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

fn save_location(location: ActionLocation, ctx: &Context<LocationsView>) {
    debug!("Saving location: {:?}", location);
    let link = ctx.link().clone();
    spawn_local(async move {
        debug!("Sending save request");

        let response = match &location {
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
                Msg::SaveFailed(location, format!("Failed to save location: {err}"))
            }
        };

        link.send_message(result);
    });
}

fn delete_location(location: Location, ctx: &Context<LocationsView>) {
    debug!("Deleting location: {:?}", location);
    let id = location.id;

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
                Msg::DeleteFailed(location, format!("Failed to delete location: {err}"))
            }
        };

        link.send_message(result);
    });
}
