use robotica_common::config::Config;
use robotica_common::mqtt::Json;
use robotica_common::mqtt::MqttMessage;
use robotica_common::version;
use std::sync::Arc;
use tracing::debug;
use tracing::error;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::components::car::CarComponent;
use crate::components::locations::zones::ZonesView;
use crate::components::nav_bar::NavBar;
use crate::components::occupancy_view::OccupancyViewComponent;
use crate::components::rooms::Room;
use crate::components::schedule_view::ScheduleView;
use crate::components::tags_view::TagsView;
use crate::components::water_heater::WaterHeaterComponent;
use crate::components::welcome::Welcome;
use crate::services::websocket::{Subscription, WebsocketService, WsEvent};
use crate::Route;

fn footer() -> Html {
    html! {
        <footer>
            <div>
                if let Some(build_date) = version::BUILD_DATE {
                    <div>{ format!("Built on {}", build_date)}</div>
                }
                if let Some(vcs_ref) = version::VCS_REF {
                    <div>{format!("VCS ref: {}", vcs_ref)}</div>
                }
            </div>
            <div>
                { "Robotica" }
            </div>
        </footer>
    }
}

fn switch(selected_route: Route) -> Html {
    let content = match selected_route {
        Route::Welcome => html! {<Welcome/>},
        Route::Room { id } => html! { <Room id={id}/> },
        Route::Car { id } => html! { <CarComponent id={id}/> },
        Route::WaterHeater { id } => html! { <WaterHeaterComponent id={id}/> },
        Route::Schedule => html! { <ScheduleView/> },
        Route::Tags => html! { <TagsView/> },
        Route::Locations => return html! { <><NavBar/><ZonesView/></> },
        Route::Occupancy => html! { <OccupancyViewComponent id={"all".to_string()}/> },
        Route::NotFound => html! {<h1>{"404 Please ask a Penguin for help"}</h1>},
    };

    html! {
        <>
            <NavBar/>
            {content}
            {footer()}
        </>
    }
}

pub enum AppMsg {
    Config(Arc<Config>),
    ConfigSubscription(Subscription),
    ConfigError(String),
    AuthEvent(WsEvent),
    AuthSubscription(Subscription),
}

pub struct App {
    wss: WebsocketService,
    config: Option<Arc<Config>>,
    config_subscription: Option<Subscription>,
    config_error: Option<String>,
    show_full_ui: bool,
    event_subscription: Option<Subscription>,
    subscribed: bool,
}

fn subscribe_to_config(ctx: &Context<App>, wss: WebsocketService, name: &str) {
    let topic = format!("robotica/config/{name}");
    let callback = ctx.link().callback(move |msg: MqttMessage| {
        debug!("Got config message");
        match msg.try_into() as Result<Json<Arc<Config>>, _> {
            Ok(Json(state)) => AppMsg::Config(state),
            Err(e) => {
                error!("Failed to deserialize config message: {e:?}");
                AppMsg::ConfigError(format!("{e:?}"))
            }
        }
    });
    let mut wss = wss;
    ctx.link().send_future(async move {
        let s = wss.subscribe_mqtt(topic, callback).await;
        AppMsg::ConfigSubscription(s)
    });
}

impl Component for App {
    type Message = AppMsg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let wss = WebsocketService::new();
        subscribe_to_config(ctx, wss.clone(), "default");

        App {
            wss,
            config: None,
            config_subscription: None,
            config_error: None,
            show_full_ui: false,
            event_subscription: None,
            subscribed: false,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            AppMsg::Config(config) => {
                debug!("Got config");
                self.config = Some(config);
                true
            }
            AppMsg::ConfigSubscription(subscription) => {
                debug!("Got config subscription");
                self.config_subscription = Some(subscription);

                false
            }
            AppMsg::ConfigError(msg) => {
                error!("Config error: {}", msg);
                self.config_error = Some(msg);
                true
            }
            AppMsg::AuthEvent(event) => {
                let show_full_ui =
                    matches!(event, WsEvent::Connected { .. } | WsEvent::Disconnected(..));
                if self.show_full_ui == show_full_ui {
                    false
                } else {
                    self.show_full_ui = show_full_ui;
                    true
                }
            }
            AppMsg::AuthSubscription(subscription) => {
                self.event_subscription = Some(subscription);
                false
            }
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, _first_render: bool) {
        if !self.subscribed {
            self.subscribed = true;
            let callback = ctx.link().callback(AppMsg::AuthEvent);
            let mut wss = self.wss.clone();
            ctx.link().send_future(async move {
                let s = wss.subscribe_events(callback).await;
                AppMsg::AuthSubscription(s)
            });
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        if let Some(error) = &self.config_error {
            return html! {
                <div class="error">
                    <h1>{"Config Error"}</h1>
                    <p>{error}</p>
                </div>
            };
        }
        if !self.show_full_ui {
            return html! {
                <ContextProvider<WebsocketService> context={self.wss.clone()}>
                    <ContextProvider<Option<Arc<Config>>> context={&self.config}>
                        <BrowserRouter>
                            <NavBar/>
                            <Welcome/>
                            {footer()}
                        </BrowserRouter>
                    </ContextProvider<Option<Arc<Config>>>>
                </ContextProvider<WebsocketService>>
            };
        }
        html! {
            <ContextProvider<WebsocketService> context={self.wss.clone()}>
                <ContextProvider<Option<Arc<Config>>> context={&self.config}>
                    <BrowserRouter>
                        <Switch<Route> render={switch}/>
                    </BrowserRouter>
                </ContextProvider<Option<Arc<Config>>>>
            </ContextProvider<WebsocketService>>
        }
    }
}
