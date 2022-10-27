//! An interactive button that receives MQTT messages
use yew::prelude::*;

use crate::services::{
    controllers::{
        get_display_state_for_action,
        lights::{self, Priority},
        music, switch, Action, ConfigTrait, ControllerTrait, DisplayState, Icon, Label,
    },
    websocket::protocol::MqttMessage,
    websocket::{Command, WebsocketService, WsEvent},
};

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
            name: props.name,
            topic_substr: props.topic_substr,
            action: props.action,
            icon: props.icon,
            scene: props.scene,
            priority: props.priority,
        };

        config.create_controller()
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
            name: props.name,
            topic_substr: props.topic_substr,
            action: props.action,
            icon: props.icon,
            play_list: props.play_list,
        };

        config.create_controller()
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
            name: config.name,
            topic_substr: config.topic_substr,
            action: config.action,
            icon: config.icon,
        };

        config.create_controller()
    }
}

/// A yew button
pub struct Button<T: ConfigTrait> {
    controller: T::Controller,
    wss: WebsocketService,
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
            name: props.name.clone(),
            topic_substr: props.topic_substr,
            action: props.action,
            icon: props.icon,
            scene: props.scene,
            priority: props.priority,
        }
    }
}

impl<T: yew::Properties + ConfigTrait + 'static> Component for Button<T> {
    type Message = Message;
    type Properties = T;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();
        let (wss, _) = ctx
            .link()
            .context::<WebsocketService>(Callback::noop())
            .expect("No context found.");

        let controller = props.create_controller();

        {
            let tx = wss.tx.clone();
            controller.get_subscriptions().iter().for_each(move |s| {
                let topic = s.topic.clone();
                let s = (*s).clone();
                // let state = state.clone();
                // let controller = controller.clone();

                let callback = ctx
                    .link()
                    .callback(move |msg: MqttMessage| Message::Receive((s.label, msg.payload)));

                let subscribe = Command::Subscribe { topic, callback };
                let mut tx_clone = tx.clone();
                tx_clone.try_send(subscribe).unwrap();
            });
        }

        {
            let callback = ctx.link().callback(Message::Event);
            let msg = Command::EventHandler(callback);
            let mut tx = wss.tx.clone();
            tx.try_send(msg).unwrap();
        }

        Button { controller, wss }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let click_callback = ctx.link().callback(|_| Message::Click);

        let icon = self.controller.get_icon();
        let name = self.controller.get_name();
        let action = self.controller.get_action();
        let display_state = self.controller.get_display_state();
        let display_state = get_display_state_for_action(display_state, &action);

        let mut classes = classes!("button");

        match display_state {
            DisplayState::HardOff => classes.push("btn-light"),
            DisplayState::Off => classes.push("btn-dark"),
            DisplayState::Error => classes.push("btn-danger"),
            DisplayState::Unknown => classes.push("btn-warning"),
            DisplayState::On => classes.push("btn-success"),
            DisplayState::OnOther => classes.push("btn-secondary"),
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
                    };
                    self.wss.tx.try_send(Command::Send(msg)).unwrap();
                }
            }
            Message::Receive((label, payload)) => {
                self.controller.process_message(label, payload);
            }
            Message::Event(WsEvent::Disconnected(_)) => self.controller.process_disconnected(),
            Message::Event(WsEvent::FatalError(_)) => self.controller.process_disconnected(),
            Message::Event(WsEvent::Connected) => {}
        }
        true
    }
}
