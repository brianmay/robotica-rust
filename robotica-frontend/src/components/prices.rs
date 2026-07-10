use std::collections::BTreeMap;

use crate::{
    components::require_connection::RequireConnection,
    services::websocket::{Subscription, WebsocketService},
};
use chrono::{DateTime, Local, NaiveDate, Utc};
use robotica_common::{
    datetime::utc_now,
    mqtt::{Json, MqttMessage},
    robotica::{
        amber::price::{ChannelType, Descriptor, IntervalType, PriceResponse},
        entities::Id,
    },
};
use yew::prelude::*;

pub enum Msg {
    SubscribedPrices(Subscription),
    Prices(Vec<PriceResponse>),
}

pub struct PricesComponent {
    prices_subscription: Option<Subscription>,
    prices: Option<Vec<PriceResponse>>,
}

fn subscribe(ctx: &Context<PricesComponent>) {
    let (wss, _): (WebsocketService, _) = ctx
        .link()
        .context(ctx.link().batch_callback(|_| None))
        .unwrap();

    let topic = Id::new("amber_account").get_state_topic("prices");
    let callback = ctx.link().callback(move |msg: MqttMessage| {
        let Json(prices): Json<Vec<PriceResponse>> = msg.try_into().unwrap();
        Msg::Prices(prices)
    });
    let mut wss = wss;
    ctx.link().send_future(async move {
        let s = wss.subscribe_mqtt(topic, callback).await;
        Msg::SubscribedPrices(s)
    });
}

const fn descriptor_to_str(descriptor: Descriptor) -> &'static str {
    match descriptor {
        Descriptor::Negative => "negative",
        Descriptor::ExtremelyLow => "extremely low",
        Descriptor::VeryLow => "very low",
        Descriptor::Low => "low",
        Descriptor::Neutral => "neutral",
        Descriptor::High => "high",
        Descriptor::Spike => "spike",
    }
}

const fn descriptor_to_class(descriptor: Descriptor) -> &'static str {
    match descriptor {
        Descriptor::Negative | Descriptor::ExtremelyLow | Descriptor::VeryLow => "table-success",
        Descriptor::Low | Descriptor::Neutral => "",
        Descriptor::High => "table-warning",
        Descriptor::Spike => "table-danger",
    }
}

fn format_price(price: &PriceResponse) -> String {
    format!("{:.1}c", price.effective_per_kwh())
}

fn format_time(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&Local).format("%H:%M").to_string()
}

#[derive(Default)]
struct IntervalSummary {
    general: Option<PriceResponse>,
    feed_in: Option<PriceResponse>,
    controlled_load: Option<PriceResponse>,
}

type DateGroup<'a> = (NaiveDate, Vec<(&'a DateTime<Utc>, &'a IntervalSummary)>);

fn summarize(prices: &[PriceResponse]) -> BTreeMap<DateTime<Utc>, IntervalSummary> {
    let mut intervals: BTreeMap<DateTime<Utc>, IntervalSummary> = BTreeMap::new();
    for price in prices {
        let summary = intervals.entry(price.start_time).or_default();
        match price.channel_type {
            ChannelType::General => summary.general = Some(price.clone()),
            ChannelType::FeedIn => summary.feed_in = Some(price.clone()),
            ChannelType::ControlledLoad => summary.controlled_load = Some(price.clone()),
        }
    }
    intervals
}

impl Component for PricesComponent {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        subscribe(ctx);
        Self {
            prices_subscription: None,
            prices: None,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SubscribedPrices(subscription) => {
                self.prices_subscription = Some(subscription);
                false
            }
            Msg::Prices(prices) => {
                self.prices = Some(prices);
                true
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <RequireConnection>
                <div class="container">
                    <h1>{ "Amber Prices" }</h1>
                    {
                        if let Some(prices) = &self.prices {
                            let now = utc_now();
                            let has_channel = |channel_type: ChannelType| {
                                prices.iter().any(|price| price.channel_type == channel_type)
                            };
                            let has_controlled_load = has_channel(ChannelType::ControlledLoad);
                            let has_feed_in = has_channel(ChannelType::FeedIn);
                            let current = prices
                                .iter()
                                .find(|price| {
                                    matches!(price.channel_type, ChannelType::General)
                                        && price.is_current(now)
                                });
                            let intervals = summarize(prices);
                            let columns = 3 + usize::from(has_controlled_load) + usize::from(has_feed_in);

                            let mut dates: Vec<DateGroup> = Vec::new();
                            for (start_time, summary) in &intervals {
                                let date = start_time.with_timezone(&Local).date_naive();
                                match dates.last_mut() {
                                    Some((last_date, list)) if *last_date == date => list.push((start_time, summary)),
                                    _ => dates.push((date, vec![(start_time, summary)])),
                                }
                            }

                            html! {
                                <>
                                {
                                    current.map_or_else(
                                        || html! { <p>{ "No current price available" }</p> },
                                        |current| html! {
                                            <table class="table table-striped">
                                                <tbody>
                                                    <tr>
                                                        <th scope="row">{ "Current Price" }</th>
                                                        <td>{ format_price(current) }</td>
                                                    </tr>
                                                    <tr>
                                                        <th scope="row">{ "Descriptor" }</th>
                                                        <td>{ descriptor_to_str(current.descriptor) }</td>
                                                    </tr>
                                                    <tr>
                                                        <th scope="row">{ "Renewables" }</th>
                                                        <td>{ format!("{:.0}%", current.renewables) }</td>
                                                    </tr>
                                                </tbody>
                                            </table>
                                        },
                                    )
                                }
                                <table class="table table-striped">
                                    <thead>
                                        <tr>
                                            <th scope="col">{ "Time" }</th>
                                            <th scope="col">{ "General" }</th>
                                            {
                                                if has_controlled_load {
                                                    html! { <th scope="col">{ "Controlled Load" }</th> }
                                                } else {
                                                    html! {}
                                                }
                                            }
                                            {
                                                if has_feed_in {
                                                    html! { <th scope="col">{ "Feed In" }</th> }
                                                } else {
                                                    html! {}
                                                }
                                            }
                                            <th scope="col">{ "Renewables" }</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {
                                            dates.iter().map(|(date, rows)| {
                                                let date_string = date.format("%A, %e %B, %Y").to_string();
                                                html! {
                                                    <>
                                                    <tr class="table-secondary">
                                                        <th scope="colgroup" colspan={columns.to_string()}>{ date_string }</th>
                                                    </tr>
                                                    {
                                                        rows.iter().map(|(start_time, summary)| {
                                                            let general = summary.general.as_ref();
                                                            let row_class = general.map_or("", |price| {
                                                                if matches!(price.interval_type, IntervalType::CurrentInterval) {
                                                                    "table-primary"
                                                                } else {
                                                                    descriptor_to_class(price.descriptor)
                                                                }
                                                            });
                                                            html! {
                                                                <tr class={row_class}>
                                                                    <td>{ format_time(**start_time) }</td>
                                                                    <td>{ general.map_or_else(String::new, format_price) }</td>
                                                                    {
                                                                        if has_controlled_load {
                                                                            html! { <td>{ summary.controlled_load.as_ref().map_or_else(String::new, format_price) }</td> }
                                                                        } else {
                                                                            html! {}
                                                                        }
                                                                    }
                                                                    {
                                                                        if has_feed_in {
                                                                            html! { <td>{ summary.feed_in.as_ref().map_or_else(String::new, format_price) }</td> }
                                                                        } else {
                                                                            html! {}
                                                                        }
                                                                    }
                                                                    <td>{ general.map_or_else(String::new, |price| format!("{:.0}%", price.renewables)) }</td>
                                                                </tr>
                                                            }
                                                        }).collect::<Html>()
                                                    }
                                                    </>
                                                }
                                            }).collect::<Html>()
                                        }
                                    </tbody>
                                </table>
                                </>
                            }
                        } else {
                            html! {
                                <p>{ "Loading..." }</p>
                            }
                        }
                    }
                </div>
            </RequireConnection>
        }
    }
}
