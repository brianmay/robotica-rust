//! An interactive button that receives MQTT messages
use robotica_common::config::Icon;
use robotica_common::robotica::lights::SceneName;
use yew::prelude::*;

use robotica_common::controllers::robotica::{hdmi, lights, music, switch};
use robotica_common::controllers::{
    tasmota, zwave, Action, ConfigTrait, ControllerTrait, DisplayState, Label,
};
use robotica_common::mqtt::MqttMessage;

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

trait ButtonPropsTrait {
    fn get_icon(&self) -> Icon;
    fn get_name(&self) -> &str;
}

/// The yew properties for a light button.
#[derive(Clone, Properties, Eq, PartialEq)]
pub struct LightProps {
    /// The name of the light.
    pub name: String,

    /// The base string that all topics are derived from.
    pub topic_substr: String,

    /// The action that the button should perform.
    pub action: Action,

    /// The icon to display on the button.
    pub icon: Icon,

    /// The scene to activate when the button is pressed.
    pub scene: SceneName,
}

impl ConfigTrait for LightProps {
    type Controller = lights::Controller;

    fn create_controller(&self) -> Self::Controller {
        let props = (*self).clone();
        let config = lights::Config {
            topic_substr: props.topic_substr,
            action: props.action,
            scene: props.scene,
        };

        config.create_controller()
    }
}

impl ButtonPropsTrait for LightProps {
    fn get_icon(&self) -> Icon {
        self.icon
    }

    fn get_name(&self) -> &str {
        self.name.as_str()
    }
}

/// The yew properties for a music button.
#[derive(Clone, Properties, Eq, PartialEq)]
pub struct Music2Props {
    /// The name of the music button.
    pub name: String,

    /// The base string that all topics are derived from.
    pub topic_substr: String,

    /// The action that the button should perform.
    pub action: Action,

    /// The icon to display on the button.
    pub icon: Icon,

    /// The play list to play when this button is pressed.
    pub play_list: String,
}

impl ConfigTrait for Music2Props {
    type Controller = music::Controller;

    fn create_controller(&self) -> Self::Controller {
        let props = (*self).clone();
        let config = music::Config {
            topic_substr: props.topic_substr,
            action: props.action,
            play_list: props.play_list,
        };

        config.create_controller()
    }
}

impl ButtonPropsTrait for Music2Props {
    fn get_icon(&self) -> Icon {
        self.icon
    }

    fn get_name(&self) -> &str {
        self.name.as_str()
    }
}

/// The yew properties for a switch button.
#[derive(Clone, Properties, Eq, PartialEq)]
pub struct SwitchProps {
    /// The name of the switch button.
    pub name: String,

    /// The base string that all topics are derived from.
    pub topic_substr: String,

    /// The action that the button should perform.
    pub action: Action,

    /// The icon to display on the button.
    pub icon: Icon,
}

impl ConfigTrait for SwitchProps {
    type Controller = switch::Controller;

    fn create_controller(&self) -> Self::Controller {
        let config = (*self).clone();
        let config = switch::Config {
            topic_substr: config.topic_substr,
            action: config.action,
        };

        config.create_controller()
    }
}

impl ButtonPropsTrait for SwitchProps {
    fn get_icon(&self) -> Icon {
        self.icon
    }

    fn get_name(&self) -> &str {
        self.name.as_str()
    }
}

/// The yew properties for a switch button.
#[derive(Clone, Properties, Eq, PartialEq)]
pub struct ZwaveProps {
    /// The name of the switch button.
    pub name: String,

    /// The base string that all topics are derived from.
    pub topic_substr: String,

    /// The action that the button should perform.
    pub action: Action,

    /// The icon to display on the button.
    pub icon: Icon,
}

impl ConfigTrait for ZwaveProps {
    type Controller = zwave::Controller;

    fn create_controller(&self) -> Self::Controller {
        let config = (*self).clone();
        let config = zwave::Config {
            topic_substr: config.topic_substr,
            action: config.action,
        };

        config.create_controller()
    }
}

impl ButtonPropsTrait for ZwaveProps {
    fn get_icon(&self) -> Icon {
        self.icon
    }

    fn get_name(&self) -> &str {
        self.name.as_str()
    }
}

/// The yew properties for a switch button.
#[derive(Clone, Properties, Eq, PartialEq)]
pub struct TasmotaProps {
    /// The name of the switch button.
    pub name: String,

    /// The base string that all topics are derived from.
    pub topic_substr: String,

    /// The action that the button should perform.
    pub action: Action,

    /// The icon to display on the button.
    pub icon: Icon,

    /// The postfix to append to the power topic.
    #[prop_or_default]
    pub power_postfix: String,
}

impl ConfigTrait for TasmotaProps {
    type Controller = tasmota::Controller;

    fn create_controller(&self) -> Self::Controller {
        let config = (*self).clone();
        let config = tasmota::Config {
            topic_substr: config.topic_substr,
            action: config.action,
            power_postfix: config.power_postfix,
        };

        config.create_controller()
    }
}

impl ButtonPropsTrait for TasmotaProps {
    fn get_icon(&self) -> Icon {
        self.icon
    }

    fn get_name(&self) -> &str {
        self.name.as_str()
    }
}

/// The yew properties for a switch button.
#[derive(Clone, Properties, Eq, PartialEq)]
pub struct HdmiProps {
    /// The name of the switch button.
    pub name: String,

    /// The base string that all topics are derived from.
    pub topic_substr: String,

    /// The action that the button should perform.
    pub action: Action,

    /// The icon to display on the button.
    pub icon: Icon,

    /// The input of the switch
    pub input: u8,

    /// The output of the switch
    pub output: u8,
}

impl ConfigTrait for HdmiProps {
    type Controller = hdmi::Controller;

    fn create_controller(&self) -> Self::Controller {
        let config = (*self).clone();
        let config = hdmi::Config {
            topic_substr: config.topic_substr,
            action: config.action,
            input: config.input,
            output: config.output,
        };

        config.create_controller()
    }
}

impl ButtonPropsTrait for HdmiProps {
    fn get_icon(&self) -> Icon {
        self.icon
    }

    fn get_name(&self) -> &str {
        self.name.as_str()
    }
}

/// A yew button
pub struct Button<T: ConfigTrait> {
    controller: T::Controller,
    subscriptions: Vec<websocket::Subscription>,
    wss: WebsocketService,
}

/// The yew message for a button.
pub enum Message {
    /// Button has been clicked
    Click,

    /// Button has received MQTT message
    Receive((Label, MqttMessage)),

    /// Button has received a `WsEvent`
    Event(WsEvent),

    /// We have subscribed to a topic
    Subscribed(websocket::Subscription),
}

impl<T: yew::Properties + ConfigTrait + ButtonPropsTrait + 'static> Component for Button<T> {
    type Message = Message;
    type Properties = T;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();
        let (wss, _): (WebsocketService, _) = ctx
            .link()
            .context(ctx.link().batch_callback(|_| None))
            .unwrap();

        let controller = props.create_controller();
        let subscriptions: Vec<websocket::Subscription> = vec![];

        {
            controller.get_subscriptions().iter().for_each(|s| {
                let topic = s.topic.clone();
                let s = (*s).clone();
                let callback = ctx
                    .link()
                    .callback(move |msg: MqttMessage| Message::Receive((s.label, msg)));

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

        let icon = ctx.props().get_icon();
        let name = ctx.props().get_name();
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
            Message::Receive((label, msg)) => {
                self.controller.process_message(label, msg);
            }
            Message::Event(WsEvent::Disconnected(_)) => self.controller.process_disconnected(),
            Message::Event(WsEvent::Connected { .. }) => {}
            Message::Subscribed(s) => {
                self.subscriptions.push(s);
            }
        }
        true
    }
}
