use super::ActionZone;
use crate::components::locations::map::{MapComponent, ParamObject};
use gloo_net::http::{Request, Response};
use gloo_net::Error;
use robotica_common::robotica::{
    http_api::ApiResponse,
    zones::{CreateZone, Zone},
};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use tap::Pipe;
use tracing::{debug, error};
use yew::{platform::spawn_local, prelude::*};

pub enum Msg {
    LoadFailed(String),
    Zones(Arc<Vec<Zone>>),
    SaveZone(ActionZone),
    SaveSuccess(Zone),
    CreateSuccess(Zone),
    SaveFailed(ActionZone, String),
    DeleteSuccess(Zone),
    DeleteFailed(Zone, String),
    CreateZone(CreateZone),
    UpdateZone(ActionZone),
    DeleteZone(ActionZone),
    RequestItem(Zone),
    RequestList,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadingStatus {
    Loading,
    Loaded,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZoneStatus {
    Unchanged,
    Changed,
    Saving,
    Error(String),
}

impl ZoneStatus {
    pub fn error(message: impl Into<String>) -> Self {
        ZoneStatus::Error(message.into())
    }

    pub const fn can_save(&self) -> bool {
        matches!(
            self,
            ZoneStatus::Unchanged | ZoneStatus::Changed | ZoneStatus::Error(_)
        )
    }
}

pub struct ZonesView {
    zone: Option<ActionZone>,
    list: Arc<Vec<Zone>>,
    status: ZoneStatus,
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

impl Component for ZonesView {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        load_list(ctx);

        Self {
            zone: None,
            status: ZoneStatus::Unchanged,
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
            Msg::Zones(zones) => {
                self.loading_status = LoadingStatus::Loaded;
                self.list = zones;
                true
            }
            Msg::CreateZone(zone) => {
                debug!("Creating zone: {:?}", zone);
                let action_zone = ActionZone::Create(zone);
                self.status = ZoneStatus::Saving;
                save_zone(action_zone, ctx);
                true
            }
            Msg::UpdateZone(zone) => {
                debug!("Updating zone: {:?}", zone);
                self.zone = Some(zone);
                self.status = ZoneStatus::Changed;
                true
            }
            Msg::DeleteZone(zone) => {
                debug!("Deleting zone: {:?}", zone);
                self.status = ZoneStatus::Saving;
                match zone {
                    ActionZone::Create(_) => self.zone = None,
                    ActionZone::Update(zone) => delete_zone(zone, ctx),
                }
                true
            }
            Msg::SaveZone(zone) => {
                if self.status.can_save() {
                    self.status = ZoneStatus::Saving;
                    save_zone(zone, ctx);
                    true
                } else {
                    debug!("Status wrong, not saving");
                    false
                }
            }
            Msg::CreateSuccess(zone) => {
                self.zone = zone.pipe(ActionZone::Update).pipe(Some);
                self.status = ZoneStatus::Unchanged;
                load_list(ctx);
                true
            }
            Msg::SaveSuccess(_zone) => {
                self.zone = None;
                self.status = ZoneStatus::Unchanged;
                load_list(ctx);
                true
            }
            Msg::SaveFailed(zone, error) => {
                self.status = ZoneStatus::error(error);
                self.zone = Some(zone);
                true
            }
            Msg::DeleteSuccess(_zone) => {
                self.zone = None;
                self.status = ZoneStatus::Unchanged;
                self.loading_status = LoadingStatus::Loading;
                load_list(ctx);
                true
            }
            Msg::DeleteFailed(_zone, error) => {
                self.status = ZoneStatus::error(error);
                true
            }
            Msg::RequestItem(zone) => {
                self.zone = Some(ActionZone::Update(zone));
                self.status = ZoneStatus::Unchanged;
                true
            }
            Msg::RequestList => {
                self.zone = None;
                self.status = ZoneStatus::Unchanged;
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let zones = self.list.clone();

        let object = if let Some(zone) = &self.zone {
            ParamObject::Item(zone.clone())
        } else {
            ParamObject::List(zones)
        };

        let create_zone = ctx.link().callback(Msg::CreateZone);
        let update_zone = ctx.link().callback(Msg::UpdateZone);
        let delete_zone = ctx.link().callback(Msg::DeleteZone);
        let save_zone = ctx.link().callback(Msg::SaveZone);
        let request_item = ctx.link().callback(Msg::RequestItem);
        let request_list = ctx.link().callback(|()| Msg::RequestList);

        html! {
            <MapComponent
                object={object}
                status={self.status.clone()}
                loading_status={self.loading_status.clone()}
                create_zone={create_zone}
                update_zone={update_zone}
                delete_zone={delete_zone}
                save_zone={save_zone}
                request_item={request_item}
                request_list={request_list}
                />
        }
    }
}

fn load_list(ctx: &Context<ZonesView>) {
    let link = ctx.link().clone();
    spawn_local(async move {
        let zones = Request::get("/api/zones")
            .send()
            .await
            .pipe(process_response::<Vec<Zone>>)
            .await;

        let result = match zones {
            Ok(zones) => Msg::Zones(Arc::new(zones)),
            Err(err) => Msg::LoadFailed(err),
        };

        link.send_message(result);
    });
}

fn save_zone(zone: ActionZone, ctx: &Context<ZonesView>) {
    debug!("Saving zone: {:?}", zone);
    let link = ctx.link().clone();
    spawn_local(async move {
        debug!("Sending save request");

        let response = match &zone {
            ActionZone::Create(zone) => Request::post("/api/zones/create")
                .json(&zone)
                .unwrap()
                .send()
                .await
                .pipe(process_response::<Zone>)
                .await
                .map(Msg::CreateSuccess),

            ActionZone::Update(zone) => Request::put("/api/zones")
                .json(&zone)
                .unwrap()
                .send()
                .await
                .pipe(process_response::<Zone>)
                .await
                .map(Msg::SaveSuccess),
        };

        let result = match response {
            Ok(response) => {
                debug!("Zone saved");
                response
            }
            Err(err) => {
                debug!("Failed to save zone: {err}");
                Msg::SaveFailed(zone, format!("Failed to save zone: {err}"))
            }
        };

        link.send_message(result);
    });
}

fn delete_zone(zone: Zone, ctx: &Context<ZonesView>) {
    debug!("Deleting zone: {:?}", zone);
    let id = zone.id;

    let link = ctx.link().clone();
    spawn_local(async move {
        debug!("Sending delete request");
        let response = Request::delete(&format!("/api/zones/{id}"))
            .send()
            .await
            .pipe(process_response::<()>)
            .await;

        let result = match response {
            Ok(()) => {
                debug!("Zone deleted");
                Msg::DeleteSuccess(zone.clone())
            }
            Err(err) => {
                debug!("Failed to delete zone: {err}");
                Msg::DeleteFailed(zone, format!("Failed to delete zone: {err}"))
            }
        };

        link.send_message(result);
    });
}
