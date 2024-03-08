use crate::components::{control::Control, map::MapComponent};
use geo::coord;
use gloo_net::http::Request;
use robotica_common::robotica::locations::Location;
use std::{ops::Deref, sync::Arc};
use tap::Pipe;
use yew::{platform::spawn_local, prelude::*};

#[derive(Clone, Debug)]
pub struct LocationWrapper(Location);

impl Eq for LocationWrapper {}

impl PartialEq for LocationWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id && self.0.name == other.0.name
    }
}

impl Deref for LocationWrapper {
    type Target = Location;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub enum Msg {
    SelectLocation(LocationWrapper),
    Locations(Arc<Vec<LocationWrapper>>),
}

pub struct LocationsView {
    location: Option<LocationWrapper>,
    locations: Arc<Vec<LocationWrapper>>,
}

impl Component for LocationsView {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let link = ctx.link().clone();
        spawn_local(async move {
            let locations = Request::get("/api/locations")
                .send()
                .await
                .unwrap()
                .json::<Vec<Location>>()
                .await
                .unwrap()
                .into_iter()
                .map(LocationWrapper)
                .collect::<Vec<_>>()
                .pipe(Arc::new);
            link.send_message(Msg::Locations(locations));
        });

        Self {
            location: None,
            locations: Arc::new(Vec::new()),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SelectLocation(location) => {
                self.location = Some(location);
            }
            Msg::Locations(locations) => {
                self.locations = locations;
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let cb = ctx.link().callback(Msg::SelectLocation);
        html! {
            <>
                <MapComponent lat={geo::Point(coord! {x: 0.0, y: 0.0})} location={self.location.clone()} locations={self.locations.clone()}  />
                <Control select_location={cb} locations={self.locations.clone()}/>
            </>
        }
    }
}
