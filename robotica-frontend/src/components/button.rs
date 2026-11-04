//! An interactive button that receives MQTT messages
use robotica_common::config::Icon;
use robotica_common::controllers::{ConfigTrait, ControllerTrait, DisplayState, ParseError};
use robotica_common::mqtt::MqttMessage;
use yew::prelude::*;

use crate::services::websocket::{self, WebsocketService, WsEvent};

#[must_use]
fn icon_to_href(icon: Icon, state: DisplayState) -> String {
    let name = match icon {
        Icon::Fan => "fan",
        Icon::Light => "light",
        Icon::Night => "night",
        Icon::Schedule => "schedule",
        Icon::Select => "select",
        Icon::Speaker => "speaker",
        Icon::Trumpet => "trumpet",
        Icon::Tv => "tv",
    };
    let version = match state {
        DisplayState::HardOff | DisplayState::Error | DisplayState::Unknown => "error",
        DisplayState::On => "on",
        DisplayState::AutoOff => "auto",
        DisplayState::Off => "off",
    };
    format!("/images/{name}_{version}.svg")
}

/// The yew properties for a button.
///
/// The properties carry the controller configuration directly, so there is
/// no duplication between the controller config and the button properties.
#[derive(Clone, Properties, Eq, PartialEq)]
pub struct ButtonProps<C: Clone + Eq + PartialEq> {
    /// The title of the button.
    pub title: String,

    /// The icon to display on the button.
    pub icon: Icon,

    /// The controller configuration for the button.
    pub config: C,
}

/// The yew message for a button.
pub enum Message<M> {
    /// Button has been clicked
    Click,

    /// Button has received an MQTT message, parsed into the controller message
    Receive(Result<M, ParseError>),

    /// Button has received a `WsEvent`
    Event(WsEvent),

    /// We have subscribed to a topic
    Subscribed(websocket::Subscription),
}

/// A yew button
pub struct Button<C: ConfigTrait> {
    controller: C::Controller,
    subscriptions: Vec<websocket::Subscription>,
    wss: WebsocketService,
}

impl<C> Component for Button<C>
where
    C: ConfigTrait + Clone + Eq + PartialEq + 'static,
    C::Controller: ControllerTrait + 'static,
{
    type Message = Message<<C::Controller as ControllerTrait>::Message>;
    type Properties = ButtonProps<C>;

    fn create(ctx: &Context<Self>) -> Self {
        let (wss, _): (WebsocketService, _) = ctx
            .link()
            .context(ctx.link().batch_callback(|_| None))
            .unwrap();

        let controller = ctx.props().config.create_controller();
        let subscriptions: Vec<websocket::Subscription> = vec![];

        {
            controller.subscriptions().iter().for_each(|s| {
                let topic = s.topic.clone();
                let parse = s.parse;
                let callback = ctx
                    .link()
                    .callback(move |msg: MqttMessage| Message::Receive(parse(msg)));

                let mut wss = wss.clone();
                ctx.link().send_future(async move {
                    let s = wss.subscribe_mqtt(topic, callback).await;
                    Message::Subscribed(s)
                });
            });
        }

        {
            let mut wss = wss.clone();
            let callback = ctx.link().callback(Message::Event);

            ctx.link().send_future(async move {
                let s = wss.subscribe_events(callback).await;
                Message::Subscribed(s)
            });
        }

        Button {
            controller,
            subscriptions,
            wss,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let click_callback = ctx.link().callback(|_| Message::Click);

        let icon = ctx.props().icon;
        let name = ctx.props().title.as_str();
        let display_state = self.controller.get_display_state();

        let state_class = match display_state {
            DisplayState::HardOff => "btn-light",
            DisplayState::Error => "btn-danger",
            DisplayState::Unknown => "btn-warning",
            DisplayState::Off => "btn-dark",
            DisplayState::On | DisplayState::AutoOff => "btn-success",
        };
        let classes = classes!("button", state_class);

        #[allow(clippy::match_same_arms)]
        let disabled = match display_state {
            DisplayState::HardOff => true,
            DisplayState::Off => false,
            DisplayState::Error => false,
            DisplayState::Unknown => false,
            DisplayState::On => false,
            DisplayState::AutoOff => false,
        };

        html! {
            <button
                class={classes}
                {disabled}
                onclick={click_callback}
            >
                <div class="icon">
                    <img src={icon_to_href(icon, display_state)}/>
                </div>
                <div>{ display_state.to_string() }</div>
                <div>{ name }</div>
            </button>
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        #[allow(clippy::match_same_arms)]
        match msg {
            Message::Click => {
                let commands = self.controller.get_press_commands();
                for msg in commands {
                    self.wss.send_mqtt(msg);
                }
            }
            Message::Receive(Ok(message)) => {
                self.controller.process_message(message);
            }
            Message::Receive(Err(err)) => {
                tracing::error!("Failed to parse message: {err}");
            }
            Message::Event(WsEvent::Disconnected(_)) => self.controller.process_disconnected(),
            Message::Event(WsEvent::LoginRequired { .. }) => self.controller.process_disconnected(),
            Message::Event(WsEvent::Connected { .. }) => {}
            Message::Subscribed(s) => {
                self.subscriptions.push(s);
            }
        }
        true
    }
}
