use yew::prelude::*;

use crate::services::{
    controllers::{
        get_display_state_for_action,
        lights::{self, Priority},
        music, switch, Action, CommonConfig, ConfigTrait, ControllerTrait, DisplayState, Icon,
        Label,
    },
    robotica::MqttMessage,
    websocket::{Command, WebsocketService, WsEvent},
};

#[derive(Clone, Properties, Eq, PartialEq)]
pub struct LightProps {
    pub name: String,
    pub topic_substr: String,
    pub action: Action,
    pub icon: Icon,
    pub scene: String,
    pub priority: Priority,
}

impl ConfigTrait for LightProps {
    type Controller = lights::Controller;
    type State = lights::State;

    fn create_controller(&self) -> Self::Controller {
        let props = (*self).clone();
        let config = lights::Config {
            c: CommonConfig {
                name: props.name,
                topic_substr: props.topic_substr,
                action: props.action,
                icon: props.icon,
            },
            scene: props.scene,
            priority: props.priority,
        };

        config.create_controller()
    }
}

#[derive(Clone, Properties, Eq, PartialEq)]
pub struct MusicProps {
    pub name: String,
    pub topic_substr: String,
    pub action: Action,
    pub icon: Icon,
    pub play_list: String,
}

impl ConfigTrait for MusicProps {
    type Controller = music::Controller;
    type State = music::State;

    fn create_controller(&self) -> Self::Controller {
        let props = (*self).clone();
        let config = music::Config {
            c: CommonConfig {
                name: props.name,
                topic_substr: props.topic_substr,
                action: props.action,
                icon: props.icon,
            },
            play_list: props.play_list,
        };

        config.create_controller()
    }
}

#[derive(Clone, Properties, Eq, PartialEq)]
pub struct SwitchProps {
    pub name: String,
    pub topic_substr: String,
    pub action: Action,
    pub icon: Icon,
}

impl ConfigTrait for SwitchProps {
    type Controller = switch::Controller;
    type State = switch::State;

    fn create_controller(&self) -> Self::Controller {
        let config = (*self).clone();
        let config = switch::Config {
            c: CommonConfig {
                name: config.name,
                topic_substr: config.topic_substr,
                action: config.action,
                icon: config.icon,
            },
        };

        config.create_controller()
    }
}

// #[function_component(LightButton)]
// pub fn light_button(props: &Props) -> Html {
//     let wss = use_context::<WebsocketService>().expect("No context found.");

//     let controller = use_state(move || {
//         let config = Config {
//             c: CommonConfig {
//                 topic_substr: props.topic_substr.clone(),
//                 action: props.action.clone(),
//                 icon: props.icon.clone(),
//             },
//             scene: props.scene.clone(),
//             priority: props.priority,
//         };

//         config.create_controller()
//     });

//     let state = {
//         let controller = controller.clone();
//         use_state(|| (*controller).new_state())
//     };

//     {
//         let controller = controller.clone();
//         let state = state.clone();

//         use_state(move || {
//             controller.get_subscriptions().iter().for_each(move |s| {
//                 let topic = s.topic.clone();
//                 let s = (*s).clone();
//                 let state = state.clone();
//                 let controller = controller.clone();

//                 let callback = callback::Callback::from(move |msg: MqttMessage| {
//                     let new_state = (*state).clone();
//                     let new_state = controller.process_message(s.label, msg.payload, new_state);
//                     state.set(new_state);
//                 });

//                 subscribe(&topic, callback);
//             });
//         });
//     }

//     let display_state = {
//         let controller = controller.clone();
//         let display_state = controller.get_display_state(&*state);
//         get_display_state_for_action(display_state, &props.action)
//     };

//     let click_callback = {
//         let controller = controller;
//         let state = state;
//         let wss = wss;

//         callback::Callback::from(move |_| {
//             let mut tx = wss.tx.clone();
//             let commands = controller.get_press_commands(&*state);
//             for c in commands {
//                 let msg = MqttMessage {
//                     topic: c.get_topic().to_string(),
//                     payload: c.get_payload().to_string(),
//                 };
//                 tx.try_send(Command::Send(msg)).unwrap();
//             }
//         })
//     };

//     html! {
//         <button
//             class="button"
//             onclick={click_callback}
//         >
//             <span class="icon">
//                 <img src={props.icon.to_href(&display_state)}/>
//                 <div>{ display_state }</div>
//             </span>
//             <span>{ &props.name }</span>
//         </button>
//     }
// }

// fn subscribe(topic: &str, callback: Callback<MqttMessage>) {
//     let wss = use_context::<WebsocketService>().expect("No context found.");

//     let topic = topic.to_string();
//     use_effect(move || {
//         let wss = wss.clone();
//         let callback = callback.clone();
//         let topic = topic;

//         let subscribe = Command::Subscribe { topic, callback };

//         let mut tx = wss.tx;
//         tx.try_send(subscribe).unwrap();

//         move || {}
//     });
// }

pub struct Button<T: ConfigTrait> {
    controller: T::Controller,
    state: T::State,
    wss: WebsocketService,
}

pub enum Message {
    Click,
    Receive((Label, String)),
    Event(WsEvent),
}

impl From<LightProps> for lights::Config {
    fn from(props: LightProps) -> Self {
        Self {
            c: CommonConfig {
                name: props.name.clone(),
                topic_substr: props.topic_substr,
                action: props.action,
                icon: props.icon,
            },
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
        let state = controller.new_state();

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

        Button {
            controller,
            state,
            wss,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let click_callback = ctx.link().callback(|_| Message::Click);

        let icon = self.controller.get_icon();
        let name = self.controller.get_name();
        let action = self.controller.get_action();
        let display_state = self.controller.get_display_state(&self.state);
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
                <span class="icon">
                    <img src={icon.to_href(&display_state)}/>
                    <div>{ display_state }</div>
                </span>
                <span>{ &name }</span>
            </button>
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Message::Click => {
                let commands = self.controller.get_press_commands(&self.state);
                for c in commands {
                    let msg = MqttMessage {
                        topic: c.get_topic().to_string(),
                        payload: c.get_payload().to_string(),
                    };
                    self.wss.tx.try_send(Command::Send(msg)).unwrap();
                }
            }
            Message::Receive((label, payload)) => {
                self.controller
                    .process_message(label, payload, &mut self.state);
            }
            Message::Event(WsEvent::Disconnect) => {
                self.controller.process_disconnected(&mut self.state)
            }
            Message::Event(WsEvent::Connect) => {}
        }
        true
    }
}
