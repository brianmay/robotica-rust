//! Event bus wrapper for `WebSocketService`
use robotica_common::mqtt::MqttMessage;
use std::collections::HashMap;
use yew::Callback;
use yew_agent::{Agent, AgentLink, Context, HandlerId};

use super::{WebsocketService, WsEvent};

/// A websocket command, sent to the websocket service.
#[derive(Debug)]
pub enum Command {
    /// Subscribe to a MQTT topic.
    Subscribe {
        /// MQTT topic to subscribe to.
        topic: String,
        /// Callback to call when a message is received.
        callback: Callback<MqttMessage>,
    },
    /// Callback to call when a connect or disconnect event occurs.
    EventHandler(Callback<WsEvent>),
    /// Send a MQTT message.
    Send(MqttMessage),
}

/// An incoming message from the websocket event bus.
pub enum Message {
    /// A message was received from the websocket service.
    ReceivedMessage(MqttMessage),

    /// A websocket event occurred.
    ReceivedEvent(WsEvent),
}

struct Subscription {
    handler: HandlerId,
    callback: Callback<MqttMessage>,
}

/// Event bus for websocket events.
pub struct EventBus {
    ws: WebsocketService,
    #[allow(dead_code)]
    link: AgentLink<EventBus>,
    subscriptions: HashMap<String, Vec<Subscription>>,
    event_callbacks: HashMap<HandlerId, Callback<WsEvent>>,
    last_message: HashMap<String, MqttMessage>,
    last_event: Option<WsEvent>,
}

impl Agent for EventBus {
    type Reach = Context<Self>;
    type Message = Message;
    type Input = Command;
    type Output = ();

    fn create(link: AgentLink<Self>) -> Self {
        let msg_callback = link.callback(Message::ReceivedMessage);
        let event_callback = link.callback(Message::ReceivedEvent);

        Self {
            ws: WebsocketService::new(msg_callback, event_callback),
            link,
            subscriptions: HashMap::new(),
            event_callbacks: HashMap::new(),
            last_message: HashMap::new(),
            last_event: None,
        }
    }

    fn update(&mut self, msg: Self::Message) {
        match msg {
            Message::ReceivedMessage(msg) => {
                if let Some(subscriptions) = self.subscriptions.get_mut(&msg.topic) {
                    for s in subscriptions.iter() {
                        s.callback.emit(msg.clone());
                    }
                }
                self.last_message.insert(msg.topic.clone(), msg);
            }
            Message::ReceivedEvent(event) => {
                if matches!(event, WsEvent::Disconnected(_)) {
                    self.last_message.clear();
                }
                self.event_callbacks.iter().for_each(|(_, cb)| {
                    cb.emit(event.clone());
                });
                self.last_event = Some(event);
            }
        }
    }

    fn handle_input(&mut self, msg: Self::Input, id: HandlerId) {
        match msg {
            Command::Subscribe { topic, callback } => {
                if let Some(last) = self.last_message.get(&topic) {
                    callback.emit(last.clone());
                }
                self.subscriptions
                    .entry(topic.clone())
                    .or_insert_with(Vec::new)
                    .push(Subscription {
                        handler: id,
                        callback,
                    });
                self.ws.subscribe(topic);
            }
            Command::EventHandler(callback) => {
                if let Some(last) = &self.last_event {
                    callback.emit(last.clone());
                }
                self.event_callbacks.insert(id, callback);
            }
            Command::Send(msg) => {
                self.ws.send(msg);
            }
        }
    }

    fn connected(&mut self, _id: HandlerId) {
        // self.subscribers.insert(id);
    }

    fn disconnected(&mut self, id: HandlerId) {
        for subs in &mut self.subscriptions.values_mut() {
            subs.retain(|sub| sub.handler != id);
        }
        self.event_callbacks.remove(&id);
    }
}

impl Drop for EventBus {
    fn drop(&mut self) {
        self.ws.close();
    }
}
