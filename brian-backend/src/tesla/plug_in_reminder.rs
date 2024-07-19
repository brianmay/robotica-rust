use chrono::Timelike;
use robotica_backend::{
    pipes::{stateful, stateless, Subscriber, Subscription},
    spawn,
};
use robotica_common::robotica::{audio::MessagePriority, message::Message};
use std::time::Duration;

use super::{private::new_message, Config, ShouldPlugin};

#[must_use]
pub fn plug_in_reminder(
    tesla: &Config,
    should_plugin_stream: stateful::Receiver<ShouldPlugin>,
) -> stateless::Receiver<Message> {
    let (message_tx, message_rx) = stateless::create_pipe("tesla_plug_in_reminder");
    let tesla = tesla.clone();

    let should_plugin_stream = should_plugin_stream.delay_repeat(
        "tesla_should_plugin (repeat)",
        Duration::from_secs(60 * 10),
        |(_, should_plugin)| *should_plugin == ShouldPlugin::ShouldPlugin,
    );

    spawn(async move {
        let mut s = should_plugin_stream.subscribe().await;
        while let Ok(should_plugin) = s.recv().await {
            let time = chrono::Local::now();
            if time.hour() >= 18 && time.hour() <= 22 && should_plugin == ShouldPlugin::ShouldPlugin
            {
                let name = &tesla.name;
                let msg = new_message(
                    format!("{name} might run away and should be leashed"),
                    MessagePriority::Low,
                );
                message_tx.try_send(msg);
            }
        }
    });

    message_rx
}
