use std::time::Duration;

use robotica_common::robotica::{message::Message, message::MessagePriority};
use robotica_tokio::{
    pipes::{delays::DelayInputOptions, stateful, Subscriber, Subscription},
    spawn,
};
use tracing::debug;

use crate::{car, reminders::ReminderService};

use super::{private::new_message, PluginAction};

pub fn plug_in_reminder(
    car: &car::Config,
    plugin_action_stream: stateful::Receiver<PluginAction>,
    reminders: &ReminderService,
) {
    let car = car.clone();
    let reminders = reminders.clone();

    // Hold plugin actions for a grace period before they reach the reminder
    // loop, so that arriving home does not create a reminder immediately and
    // the user has time to plug in. A later action clears the pending one, so
    // plugging in during the grace period means the reminder is never
    // created; actions without plugin set pass through undelayed.
    let plugin_action_stream = plugin_action_stream.delay_input(
        "tesla_should_plugin (delay)",
        Duration::from_mins(10),
        |(_, action)| action.plugin,
        DelayInputOptions::default(),
    );

    spawn(async move {
        let mut s = plugin_action_stream.subscribe().await;
        let mut auto_reminder: Option<crate::reminders::Reminder> = None;
        let mut plugin_reminder: Option<crate::reminders::Reminder> = None;

        while let Ok(action) = s.recv().await {
            if action.switch_to_auto {
                if auto_reminder.is_none() {
                    let name = &car.name;
                    let msg: Message = new_message(
                        format!("{name} should be automatic"),
                        MessagePriority::Error,
                        &car.audience.charging,
                    );
                    let key = format!("{}/auto", car.id);
                    debug!(reminder_key = %key, "Creating auto reminder");
                    auto_reminder = Some(reminders.create(key, msg));
                }
            } else if auto_reminder.take().is_some() {
                debug!("Auto reminder is no longer active");
            }

            if action.plugin {
                if plugin_reminder.is_none() {
                    let name = &car.name;
                    let msg: Message = new_message(
                        format!("{name} might fly away and should be leashed"),
                        MessagePriority::Error,
                        &car.audience.charging,
                    );
                    let key = format!("{}/plug_in", car.id);
                    debug!(reminder_key = %key, "Creating plug-in reminder");
                    plugin_reminder = Some(reminders.create(key, msg));
                }
            } else if plugin_reminder.take().is_some() {
                debug!("Plug-in reminder is no longer active");
            }
        }
    });
}
