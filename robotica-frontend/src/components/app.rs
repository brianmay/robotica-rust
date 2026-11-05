use std::sync::Arc;

use robotica_common::config::{view_id_to_entity, Config, ViewConfig, ViewKind};
use robotica_common::version;
use tracing::{debug, error};
use yew::prelude::*;
use yew_router::prelude::*;

use crate::components::car::CarComponent;
use crate::components::controls::Controls;
use crate::components::locations::zones::ZonesView;
use crate::components::nav_bar::NavBar;
use crate::components::occupancy_view::OccupancyViewComponent;
use crate::components::prices::PricesComponent;
use crate::components::schedule_view::ScheduleView;
use crate::components::tags_view::TagsView;
use crate::components::water_heater::WaterHeaterComponent;
use crate::components::welcome::Welcome;
use crate::services::api;
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
        Route::View { id } => html! { <View {id}/> },
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

/// Properties for a view.
#[derive(Properties, Eq, PartialEq, Clone)]
struct ViewProps {
    /// The id of the view to render.
    id: String,
}

/// Look up a view in the config and render it.
///
/// This is the single dispatcher from a view id to the component that
/// renders it.
#[function_component(View)]
fn view(props: &ViewProps) -> Html {
    let config = match use_context::<Option<Arc<Config>>>() {
        Some(Some(config)) => config,
        Some(None) => {
            return html! {
                <h1>{"Loading..."}</h1>
            };
        }
        None => {
            return html! {
                <h1>{"Config error..."}</h1>
            };
        }
    };

    let Some(view) = config.get_view(&props.id) else {
        return html!(<h1>{"404 Please ask a Penguin for help"}</h1>);
    };

    render_view(view)
}

/// Render a resolved view.
///
/// This is an exhaustive match: a new view kind cannot be added without
/// also rendering it here.
fn render_view(view: &ViewConfig) -> Html {
    match &view.kind {
        ViewKind::Controls(controls) => html! {
            <Controls id={view.id.clone()} title={view.title.clone()} rows={controls.rows.clone()} />
        },
        ViewKind::Car => match view_id_to_entity(&view.id) {
            Ok(id) => html! { <CarComponent {id} title={view.title.clone()} /> },
            Err(e) => html! { <h1>{ format!("404 invalid id: {e}") }</h1> },
        },
        ViewKind::WaterHeater => match view_id_to_entity(&view.id) {
            Ok(id) => html! { <WaterHeaterComponent {id} title={view.title.clone()} /> },
            Err(e) => html! { <h1>{ format!("404 invalid id: {e}") }</h1> },
        },
        ViewKind::Schedule => html! { <ScheduleView/> },
        ViewKind::Tags => html! { <TagsView/> },
        ViewKind::Prices => html! { <PricesComponent/> },
        ViewKind::Locations(locations) => html! {
            <ZonesView objects={locations.objects.clone()} />
        },
        ViewKind::Occupancy(occupancy) => html! {
            <OccupancyViewComponent people={occupancy.people.clone()} />
        },
    }
}

pub enum AppMsg {
    Config(Arc<Config>),
    ConfigError(String),
    AuthEvent(WsEvent),
    AuthSubscription(Subscription),
}

pub struct App {
    wss: WebsocketService,
    config: Option<Arc<Config>>,
    config_error: Option<String>,
    show_full_ui: bool,
    event_subscription: Option<Subscription>,
    subscribed: bool,
}

/// Fetch the per user UI configuration over HTTP.
///
/// The backend filters the default profile to the views the user's groups
/// allow; the navbar renders exactly those views. Called once the
/// websocket reports a successful (authenticated) connection.
fn fetch_config(ctx: &Context<App>) {
    let link = ctx.link().clone();
    yew::platform::spawn_local(async move {
        let result = api::get_ui_config().await;
        let msg = match result {
            Ok(config) => AppMsg::Config(Arc::new(config)),
            Err(err) => AppMsg::ConfigError(err),
        };
        link.send_message(msg);
    });
}

impl Component for App {
    type Message = AppMsg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        let wss = WebsocketService::new();

        App {
            wss,
            config: None,
            config_error: None,
            show_full_ui: false,
            event_subscription: None,
            subscribed: false,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            AppMsg::Config(config) => {
                debug!("Got config");
                self.config = Some(config);
                true
            }
            AppMsg::ConfigError(msg) => {
                error!("Config error: {}", msg);
                self.config_error = Some(msg);
                true
            }
            AppMsg::AuthEvent(event) => {
                if matches!(event, WsEvent::Connected { .. }) {
                    // The websocket only connects for authenticated
                    // sessions, so it is now safe to fetch the per user
                    // ui config.
                    fetch_config(ctx);
                }

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
