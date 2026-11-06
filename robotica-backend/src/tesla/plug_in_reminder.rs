use chrono::Timelike;
use robotica_common::robotica::{message::Message, message::MessagePriority};
use robotica_tokio::{
    pipes::{stateful, Subscriber, Subscription},
    spawn,
};
use std::time::Duration;
use tracing::debug;

use crate::{car, reminders::ReminderService};

use super::{private::new_message, ShouldPlugin};

pub fn plug_in_reminder(
    car: &car::Config,
    should_plugin_stream: stateful::Receiver<ShouldPlugin>,
    reminders: &ReminderService,
) {
    let car = car.clone();
    let reminders = reminders.clone();

    let should_plugin_stream = should_plugin_stream.delay_repeat(
        "tesla_should_plugin (repeat)",
        Duration::from_mins(10),
        |(_, should_plugin)| *should_plugin == ShouldPlugin::ShouldPlugin,
    );

    spawn(async move {
        let mut s = should_plugin_stream.subscribe().await;
        let mut reminder: Option<crate::reminders::Reminder> = None;

        while let Ok(should_plugin) = s.recv().await {
            let time = chrono::Local::now();
            let in_window = (18..=22).contains(&time.hour());
            let active = in_window && should_plugin == ShouldPlugin::ShouldPlugin;

            if active {
                if reminder.is_none() {
                    let name = &car.name;
                    let msg: Message = new_message(
                        format!("{name} might fly away and should be leashed"),
                        MessagePriority::Error,
                        &car.audience.charging,
                    );
                    let key = format!("{}/plug_in", car.id);
                    debug!(reminder_key = %key, "Creating plug-in reminder");
                    reminder = Some(reminders.create(key, msg));
                }
            } else if reminder.take().is_some() {
                debug!("Plug-in reminder is no longer active");
            }
        }
    });
}
