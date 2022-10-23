use log::{debug, error};
use web_sys::HtmlInputElement;
use yew::prelude::*;

use crate::services::{
    robotica::MqttMessage,
    websocket::{Command, WebsocketService},
};

pub enum Msg {
    HandleMsg(MqttMessage),
    SubmitMessage,
}

pub struct Chat {
    // users: Vec<UserProfile>,
    chat_input: NodeRef,
    wss: WebsocketService,
    messages: Vec<MqttMessage>,
}
impl Component for Chat {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let callback = ctx.link().callback(Msg::HandleMsg);
        let subscribe = Command::Subscribe {
            topic: "test".to_string(),
            callback,
        };
        let (wss, _) = ctx
            .link()
            .context::<WebsocketService>(Callback::noop())
            .expect("Context to be set");
        wss.tx.clone().try_send(subscribe).unwrap();

        Self {
            // users: vec![],
            messages: vec![],
            chat_input: NodeRef::default(),
            wss,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let submit = ctx.link().callback(|_| Msg::SubmitMessage);
        html! {
            <div class="flex w-screen">
                <div class="grow h-screen flex flex-col">
                    <div class="w-full grow overflow-auto border-b-2 border-gray-300">
                        {
                            self.messages.iter().map(|m| {
                                html! {
                                    <div class="flex flex-row">
                                        <div class="p-2">{m.topic.clone()}</div>
                                        <div class="p-2">{m.payload.clone()}</div>
                                    </div>
                                }
                            }).collect::<Html>()
                        }
                    </div>
                    <div class="w-full h-14 flex px-3 items-center">
                        <input ref={self.chat_input.clone()} type="text" placeholder="Message" class="block w-full py-2 pl-4 mx-3 bg-gray-100 rounded-full outline-none focus:text-gray-700" name="message" required=true />
                        <button onclick={submit} class="p-3 shadow-sm bg-blue-600 w-10 h-10 rounded-full flex justify-center items-center color-white">
                            <svg fill="#000000" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" class="fill-white">
                                <path d="M0 0h24v24H0z" fill="none"></path><path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z"></path>
                            </svg>
                        </button>
                    </div>
                </div>
            </div>
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::HandleMsg(msg) => {
                debug!("Got message: {:?}", msg);
                self.messages.push(msg);
                true
            }
            Msg::SubmitMessage => {
                let input = self.chat_input.cast::<HtmlInputElement>();
                if let Some(input) = input {
                    let message = MqttMessage {
                        topic: "test".to_string(),
                        payload: input.value(),
                    };
                    let command = Command::Send(message);
                    if let Err(e) = self.wss.tx.try_send(command) {
                        error!("error sending to channel: {:?}", e);
                    }
                    input.set_value("");
                };
                false
            }
        }
    }
}
