use std::sync::Arc;

use crate::{
    components::require_connection::RequireConnection,
    services::websocket::{Subscription, WebsocketService},
};
use robotica_common::{
    config::Config,
    datetime::{datetime_to_time_string, time_delta},
    mqtt::{Json, MqttMessage},
    robotica::{amber, entities::Id},
};
use tracing::debug;
use yew::prelude::*;

pub enum Msg {
    SubscribedState(Subscription),
    Config(Option<Arc<Config>>),
    State(amber::hot_water::State),
}

#[derive(Eq, PartialEq, Properties, Clone)]
pub struct Props {
    pub id: String,
}

pub struct HotWaterComponent {
    state_subscription: Option<Subscription>,
    state: Option<amber::hot_water::State>,
    config: Option<Arc<Config>>,
    _config_handle: ContextHandle<Option<Arc<Config>>>,
}

fn subscribe_to_car(ctx: &Context<HotWaterComponent>, car_id: &Id) {
    let (wss, _): (WebsocketService, _) = ctx
        .link()
        .context(ctx.link().batch_callback(|_| None))
        .unwrap();

    let topic = car_id.get_state_topic("amber");
    let callback = ctx.link().callback(move |msg: MqttMessage| {
        let Json(state): Json<amber::hot_water::State> = msg.try_into().unwrap();
        Msg::State(state)
    });
    let mut wss = wss;
    ctx.link().send_future(async move {
        let s = wss.subscribe_mqtt(topic, callback).await;
        Msg::SubscribedState(s)
    });
}

impl Component for HotWaterComponent {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (config, config_handle): (Option<Arc<Config>>, _) = ctx
            .link()
            .context(ctx.link().callback(Msg::Config))
            .unwrap();

        let props = ctx.props();
        let id = Id::new(&props.id);

        subscribe_to_car(ctx, &id);
        Self {
            state_subscription: None,
            state: None,
            config,
            _config_handle: config_handle,
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {}
    }

    #[allow(clippy::cognitive_complexity)]
    #[allow(clippy::too_many_lines)]
    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SubscribedState(subscription) => {
                self.state_subscription = Some(subscription);
                false
            }
            Msg::State(state) => {
                debug!("Hot Water state: {:?}", state);
                self.state = Some(state);
                true
            }
            Msg::Config(config) => {
                self.config = config;
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let id = Id::new(&props.id);

        let hot_water = if let Some(config) = &self.config {
            config.cars.iter().find(|car| car.id == id).cloned()
        } else {
            None
        };
        let title = hot_water
            .as_ref()
            .map_or("Unknown", |hot_water| hot_water.title.as_str());

        html! {
            <RequireConnection>
                <div>
                   {  if let Some(state) = &self.state {
                        html! {
                            <div>
                                <h1>{ format!("Hot Water: {title}") }</h1>
                                <p>{ format!("State: {:?}", state) }</p>
                                 <p> { "result:" } { state.get_result() } </p>

                                {
                                    if let Some(plan) = state.combined.get_plan().get() {
                                        html!{ <>
                                        <p> { "start:" } { datetime_to_time_string(plan.get_start_time()) } </p>
                                        <p> { "end:" } { datetime_to_time_string(plan.get_end_time()) } </p>
                                        <p> { "request:" } { plan.get_request() } </p>
                                        <p> { "timedelta:" } { time_delta::to_string(plan.get_timedelta()) } </p>
                                        </>}
                                    } else {
                                        html!{ <p> { "No plan" } </p> }
                                    }
                               }
                            </div>
                        }
                    } else {
                        html! {
                            <p>{ "Loading..." }</p>
                        }
                    } }
                </div>
            </RequireConnection>
        }
    }
}
