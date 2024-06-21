use chrono::Timelike;
use robotica_backend::{
    pipes::{stateful, Subscriber, Subscription},
    spawn,
};
use robotica_common::robotica::audio::MessagePriority;
use std::time::Duration;

use crate::{delays::delay_repeat, InitState};

use super::{private::new_message, Config, ShouldPlugin};

pub fn plug_in_reminder(
    state: &InitState,
    tesla: &Config,
    should_plugin_stream: stateful::Receiver<ShouldPlugin>,
) {
    let message_sink = state.message_sink.clone();
    let tesla = tesla.clone();

    let should_plugin_stream = delay_repeat(
        "tesla_should_plugin (repeat)",
        Duration::from_secs(60 * 10),
        should_plugin_stream,
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
                message_sink.try_send(msg);
            }
        }
    });
}
