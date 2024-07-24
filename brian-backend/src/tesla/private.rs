use robotica_common::robotica::{
    audio::MessagePriority,
    message::{Audience, Message},
};

pub fn new_message(
    message: impl Into<String>,
    priority: MessagePriority,
    audience: impl Into<Audience>,
) -> Message {
    Message::new("Tesla", message.into(), priority, audience)
}
