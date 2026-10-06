use robotica_common::robotica::message::{Audience, Message, MessagePriority};

pub fn new_message(
    message: impl Into<String>,
    priority: MessagePriority,
    audience: impl Into<Audience>,
) -> Message {
    Message::new("Tesla", message.into(), priority, audience)
}
