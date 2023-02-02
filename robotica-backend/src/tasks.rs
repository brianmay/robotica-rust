//! Backend specific behaviour for tasks.
use robotica_common::{
    mqtt::MqttMessage,
    robotica::tasks::{Payload, Task},
};

/// Get the MQTT message for this task.
#[must_use]
pub fn get_task_messages(task: &Task) -> Vec<MqttMessage> {
    let mut messages = Vec::with_capacity(task.topics.len());
    for topic in &task.topics {
        let payload = match &task.payload {
            Payload::String(s) => s.to_string(),
            Payload::Json(v) => v.to_string(),
        };
        let message = MqttMessage::new(topic, payload, task.retain, task.qos);
        messages.push(message);
    }
    messages
}
