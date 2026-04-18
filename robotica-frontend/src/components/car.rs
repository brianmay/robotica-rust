use std::sync::Arc;
use wasm_bindgen::JsCast;

use crate::{
    components::{
        button::{Button, SwitchProps},
        require_connection::RequireConnection,
    },
    services::websocket::{Subscription, WebsocketService},
};
use robotica_common::{
    config::{Config, Icon},
    controllers::Action,
    datetime::{datetime_to_time_string, time_delta},
    mqtt::{Json, MqttMessage, QoS, Retain},
    robotica::{amber, entities::Id},
};
use tracing::debug;
use yew::prelude::*;

pub enum Msg {
    SubscribedState(Subscription),
    Config(Option<Arc<Config>>),
    State(amber::car::State),
    EditMinCharge(String),
    StartEdit,
    CancelEdit,
    SaveMinCharge,
}

#[derive(Eq, PartialEq, Properties, Clone)]
pub struct Props {
    pub id: String,
}

pub struct CarComponent {
    state_subscription: Option<Subscription>,
    state: Option<amber::car::State>,
    config: Option<Arc<Config>>,
    _config_handle: ContextHandle<Option<Arc<Config>>>,
    edit_min_charge: String,
    is_editing: bool,
    edit_error: Option<String>,
    wss: WebsocketService,
}

fn subscribe(ctx: &Context<CarComponent>, car_id: &Id, wss: WebsocketService) {
    let topic = car_id.get_state_topic("amber");
    let callback = ctx.link().callback(move |msg: MqttMessage| {
        let Json(state): Json<amber::car::State> = msg.try_into().unwrap();
        Msg::State(state)
    });
    let mut wss = wss;
    ctx.link().send_future(async move {
        let s = wss.subscribe_mqtt(topic, callback).await;
        Msg::SubscribedState(s)
    });
}

impl Component for CarComponent {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (config, config_handle): (Option<Arc<Config>>, _) = ctx
            .link()
            .context(ctx.link().callback(Msg::Config))
            .unwrap();

        let (wss, _): (WebsocketService, _) = ctx
            .link()
            .context(ctx.link().batch_callback(|_| None))
            .unwrap();

        let props = ctx.props();
        let id = Id::new(&props.id);

        subscribe(ctx, &id, wss.clone());
        Self {
            state_subscription: None,
            state: None,
            config,
            _config_handle: config_handle,
            edit_min_charge: String::new(),
            is_editing: false,
            edit_error: None,
            wss,
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {}
    }

    #[allow(clippy::cognitive_complexity)]
    #[allow(clippy::too_many_lines)]
    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SubscribedState(subscription) => {
                self.state_subscription = Some(subscription);
                false
            }
            Msg::State(state) => {
                debug!("Car state: {:?}", state);
                self.state = Some(state);
                true
            }
            Msg::Config(config) => {
                self.config = config;
                true
            }
            Msg::EditMinCharge(value) => {
                self.edit_min_charge = value;
                true
            }
            Msg::StartEdit => {
                self.is_editing = true;
                self.edit_error = None;
                if let Some(state) = &self.state {
                    self.edit_min_charge = state.min_charge_tomorrow.to_string();
                }
                true
            }
            Msg::CancelEdit => {
                self.is_editing = false;
                self.edit_min_charge = String::new();
                self.edit_error = None;
                true
            }
            Msg::SaveMinCharge => {
                if let Ok(value) = self.edit_min_charge.parse::<u8>() {
                    if value <= 100 {
                        let props = ctx.props();
                        let id = Id::new(&props.id);
                        let topic = id.get_command_topic("min_charge_tomorrow");
                        let msg = MqttMessage::new(
                            &topic,
                            self.edit_min_charge.clone(),
                            Retain::NoRetain,
                            QoS::ExactlyOnce,
                        );
                        self.wss.send_mqtt(msg);
                        self.is_editing = false;
                        self.edit_error = None;
                        return false;
                    }
                }
                self.edit_error = Some("Please enter a number between 0 and 100".to_string());
                true
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let id = Id::new(&props.id);

        let car = if let Some(config) = &self.config {
            config.cars.iter().find(|car| car.id == id).cloned()
        } else {
            None
        };
        let title = car.as_ref().map_or("Unknown", |car| car.title.as_str());

        let switch_props = SwitchProps {
            name: "Charge".to_string(),
            icon: Icon::Light,
            action: Action::Toggle,
            topic_substr: format!("{id}/auto_charge"),
        };

        html! {
            <RequireConnection>
                <div class="container">
                   {  if let Some(state) = &self.state {
                        html! {
                            <div>
                                <h1>{ format!("Car: {title}") }</h1>
                                <table class="table table-striped">
                                    <tbody>
                                        <tr>
                                            <th scope="row">{"Battery Level"}</th>
                                            <td>{ state.battery_level }{ "%" }</td>
                                        </tr>
                                        <tr>
                                            <th scope="row">{"Min Charge Tomorrow"}</th>
                                            <td>
                                                {
                                                    if self.is_editing {
                                                        html! {
                                                            <>
                                                                <input
                                                                    type="number"
                                                                    class="form-control"
                                                                    min="0"
                                                                    max="100"
                                                                    value={self.edit_min_charge.clone()}
                                                                    oninput={ctx.link().callback(|e: InputEvent| {
                                                                        let value = e.target()
                                                                            .unwrap()
                                                                            .unchecked_into::<web_sys::HtmlInputElement>()
                                                                            .value();
                                                                        Msg::EditMinCharge(value)
                                                                    })}
                                                                />
                                                                <button
                                                                    class="btn btn-primary btn-sm mt-2"
                                                                    onclick={ctx.link().callback(|_| Msg::SaveMinCharge)}
                                                                >
                                                                    { "Save" }
                                                                </button>
                                                                <button
                                                                    class="btn btn-secondary btn-sm mt-2 ms-2"
                                                                    onclick={ctx.link().callback(|_| Msg::CancelEdit)}
                                                                >
                                                                    { "Cancel" }
                                                                </button>
                                                                {
                                                                    if let Some(error) = &self.edit_error {
                                                                        html! {
                                                                            <div class="text-danger mt-2">{ error }</div>
                                                                        }
                                                                    } else {
                                                                        html! {}
                                                                    }
                                                                }
                                                            </>
                                                        }
                                                    } else {
                                                        html! {
                                                            <>
                                                                { state.min_charge_tomorrow }{ "%" }
                                                                <button
                                                                    class="btn btn-secondary btn-sm ms-2"
                                                                    onclick={ctx.link().callback(|_| Msg::StartEdit)}
                                                                >
                                                                    { "Edit" }
                                                                </button>
                                                            </>
                                                        }
                                                    }
                                                }
                                            </td>
                                        </tr>
                                        <tr>
                                            <th scope="row">{"Current Result"}</th>
                                            <td>{ state.get_result().to_string() }</td>
                                        </tr>
                                        {
                                            if let Some(plan) = state.combined.get_plan().get() {
                                                html!{ <>
                                                <tr>
                                                    <th scope="row">{"Plan Start"}</th>
                                                    <td>{ datetime_to_time_string(plan.get_start_time()) }</td>
                                                </tr>
                                                <tr>
                                                    <th scope="row">{"Plan End"}</th>
                                                    <td>{ datetime_to_time_string(plan.get_end_time()) }</td>
                                                </tr>
                                                <tr>
                                                    <th scope="row">{"Plan Result"}</th>
                                                    <td>{ plan.get_request().to_string() }</td>
                                                </tr>
                                                <tr>
                                                    <th scope="row">{"Plan Duration"}</th>
                                                    <td>{ time_delta::to_string(plan.get_timedelta()) }</td>
                                                </tr>
                                                <tr>
                                                    <th scope="row">{"Plan Cost"}</th>
                                                    <td>{ format!("{:.2}p", plan.get_total_cost()) }</td>
                                                </tr>
                                                </>}
                                            } else {
                                                html!{ <>
                                                <tr>
                                                    <th scope="row">{"Plan"}</th>
                                                    <td>{ "No active plan" }</td>
                                                </tr>
                                                </>}
                                            }
                                        }
                                        <tr>
                                            <th scope="row">{"Rules"}</th>
                                            <td>
                                                <ul class="list-unstyled mb-0">
                                                    {
                                                        state.combined.get_rules().get_rules().iter().map(|rule| {
                                                            html! {
                                                                <li>{ rule.get_condition() }{ ": " }{ rule.get_result().to_string() }</li>
                                                            }
                                                        }).collect::<Html>()
                                                    }
                                                </ul>
                                            </td>
                                        </tr>
                                        <tr>
                                            <th scope="row">{"Rules Context"}</th>
                                            <td>
                                                <ul class="list-unstyled mb-0">
                                                    <li>{ "Day of week: " }{ state.combined.get_rules_context().get_day_of_week() }</li>
                                                    <li>{ "Hour: " }{ state.combined.get_rules_context().get_hour() }</li>
                                                    <li>{ "Current price: " }{ format!("{:.2}p", state.combined.get_rules_context().get_current_price()) }</li>
                                                    <li>{ "Weighted price: " }{ format!("{:.2}p", state.combined.get_rules_context().get_weighted_price()) }</li>
                                                    <li>{ "Is on: " }{ state.combined.get_rules_context().get_is_on().to_string() }</li>
                                                </ul>
                                            </td>
                                        </tr>
                                        {
                                            if let Some(rules_result) = state.combined.get_rules_result() {
                                                html!{ <>
                                                <tr>
                                                    <th scope="row">{"Rules Result"}</th>
                                                    <td>{ rules_result.to_string() }</td>
                                                </tr>
                                                </>}
                                            } else {
                                                html! {}
                                            }
                                        }
                                    </tbody>
                                </table>
                            </div>
                        }
                    } else {
                        html! {
                            <p>{ "Loading..." }</p>
                        }
                    } }
                </div>
                <div class="mt-3">
                  <Button<SwitchProps> ..switch_props />
                </div>
            </RequireConnection>
        }
    }
}
