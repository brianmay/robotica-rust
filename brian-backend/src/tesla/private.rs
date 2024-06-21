use robotica_common::robotica::{audio::MessagePriority, message::Message};

use crate::audience;

pub fn new_message(message: impl Into<String>, priority: MessagePriority) -> Message {
    Message::new("Tesla", message.into(), priority, audience::everyone())
}

pub fn new_private_message(message: impl Into<String>, priority: MessagePriority) -> Message {
    Message::new("Tesla", message.into(), priority, audience::brian(true))
}
