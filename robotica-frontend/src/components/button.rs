//! An interactive button that receives MQTT messages
use yew::prelude::*;

use robotica_common::mqtt::MqttMessage;
use yew_agent::{Bridge, Bridged};

use crate::services::{
    controllers::{
        get_display_state_for_action, hdmi,
        lights::{self, Priority},
        music, switch, Action, ConfigTrait, ControllerTrait, DisplayState, Label,
    },
    icons::Icon,
    websocket::{
        event_bus::{Command, EventBus},
        WsEvent,
    },
};

trait ButtonPropsTrait {
    fn get_icon(&self) -> &Icon;
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
    pub scene: String,

    /// The priority of the scene.
    pub priority: Priority,
}

impl ConfigTrait for LightProps {
    type Controller = lights::Controller;

    fn create_controller(&self) -> Self::Controller {
        let props = (*self).clone();
        let config = lights::Config {
            topic_substr: props.topic_substr,
            action: props.action,
            scene: props.scene,
            priority: props.priority,
        };

        config.create_controller()
    }
}

impl ButtonPropsTrait for LightProps {
    fn get_icon(&self) -> &Icon {
        &self.icon
    }

    fn get_name(&self) -> &str {
        self.name.as_str()
    }
}

/// The yew properties for a music button.
#[derive(Clone, Properties, Eq, PartialEq)]
pub struct MusicProps {
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

impl ConfigTrait for MusicProps {
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

impl ButtonPropsTrait for MusicProps {
    fn get_icon(&self) -> &Icon {
        &self.icon
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
    fn get_icon(&self) -> &Icon {
        &self.icon
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
    fn get_icon(&self) -> &Icon {
        &self.icon
    }

    fn get_name(&self) -> &str {
        self.name.as_str()
    }
}

/// A yew button
pub struct Button<T: ConfigTrait> {
    controller: T::Controller,
    events: Box<dyn Bridge<EventBus>>,
}

/// The yew message for a button.
pub enum Message {
    /// Button has been clicked
    Click,

    /// Button was received MQTT message
    Receive((Label, String)),

    /// Button was received a WebSocket event
    Event(WsEvent),
}

impl From<LightProps> for lights::Config {
    fn from(props: LightProps) -> Self {
        Self {
            topic_substr: props.topic_substr,
            action: props.action,
            scene: props.scene,
            priority: props.priority,
        }
    }
}

impl<T: yew::Properties + ConfigTrait + ButtonPropsTrait + 'static> Component for Button<T> {
    type Message = Message;
    type Properties = T;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();
        let mut events = EventBus::bridge(ctx.link().batch_callback(|_| None));
        let controller = props.create_controller();

        {
            controller.get_subscriptions().iter().for_each(|s| {
                let topic = s.topic.clone();
                let s = (*s).clone();
                let callback = ctx
                    .link()
                    .callback(move |msg: MqttMessage| Message::Receive((s.label, msg.payload)));

                let subscribe = Command::Subscribe { topic, callback };
                events.send(subscribe);
            });
        }

        {
            let callback = ctx.link().callback(Message::Event);
            let msg = Command::EventHandler(callback);
            events.send(msg);
        }

        Button { controller, events }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let click_callback = ctx.link().callback(|_| Message::Click);

        let icon = ctx.props().get_icon();
        let name = ctx.props().get_name();
        let action = self.controller.get_action();
        let display_state = self.controller.get_display_state();
        let display_state = get_display_state_for_action(display_state, &action);

        let mut classes = classes!("button");

        match display_state {
            DisplayState::HardOff => classes.push("btn-light"),
            DisplayState::Error => classes.push("btn-danger"),
            DisplayState::Unknown => classes.push("btn-warning"),
            DisplayState::Off | DisplayState::OnOther => classes.push("btn-dark"),
            DisplayState::On => classes.push("btn-success"),
        }

        #[allow(clippy::match_same_arms)]
        let disabled = match display_state {
            DisplayState::HardOff => true,
            DisplayState::Off => false,
            DisplayState::Error => false,
            DisplayState::Unknown => false,
            DisplayState::On => false,
            DisplayState::OnOther => false,
        };

        html! {
            <button
                class={classes}
                {disabled}
                onclick={click_callback}
            >
                <div class="icon">
                    <img src={icon.to_href(&display_state)}/>
                </div>
                <div>{ display_state }</div>
                <div>{ &name }</div>
            </button>
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        #[allow(clippy::match_same_arms)]
        match msg {
            Message::Click => {
                let commands = self.controller.get_press_commands();
                for c in commands {
                    let msg = MqttMessage {
                        topic: c.get_topic().to_string(),
                        payload: c.get_payload().to_string(),
                        ..Default::default()
                    };
                    self.events.send(Command::Send(msg));
                }
            }
            Message::Receive((label, payload)) => {
                self.controller.process_message(label, payload);
            }
            Message::Event(WsEvent::Disconnected(_)) => self.controller.process_disconnected(),
            Message::Event(WsEvent::Connected { .. }) => {}
        }
        true
    }
}
