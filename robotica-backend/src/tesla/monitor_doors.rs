use std::fmt::Display;

use robotica_common::{
    mqtt::{BoolError, MqttMessage},
    robotica::message::{Message, MessagePriority},
};
use robotica_tokio::{
    pipes::{delays::DelayInputOptions, stateful, stateless, Subscriber, Subscription},
    spawn,
};
use std::time::Duration;
use thiserror::Error;
use tokio::select;
use tracing::{debug, error};

use crate::{car, reminders::ReminderService, tesla::private::new_message};

use super::Receivers;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DoorState {
    Open,
    Closed,
}

impl Display for DoorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::Closed => write!(f, "closed"),
        }
    }
}

impl TryFrom<MqttMessage> for DoorState {
    type Error = StateErr;
    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        match msg.try_into() {
            Ok(true) => Ok(Self::Open),
            Ok(false) => Ok(Self::Closed),
            Err(err) => Err(err.into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserIsPresent {
    UserPresent,
    UserNotPresent,
}

impl Display for UserIsPresent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserPresent => write!(f, "user is present"),
            Self::UserNotPresent => write!(f, "user is not present"),
        }
    }
}

impl TryFrom<MqttMessage> for UserIsPresent {
    type Error = StateErr;
    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        match msg.try_into() {
            Ok(true) => Ok(Self::UserPresent),
            Ok(false) => Ok(Self::UserNotPresent),
            Err(err) => Err(err.into()),
        }
    }
}

#[derive(Error, Debug)]
pub enum StateErr {
    #[error("Invalid door state: {0}")]
    InvalidDoorState(#[from] BoolError),

    #[error("Invalid UTF8")]
    Utf8Error(#[from] std::str::Utf8Error),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Door {
    Frunk,
    Boot,
    Doors,
    #[allow(dead_code)]
    Windows,
}

impl Door {
    const fn is_plural(&self) -> bool {
        match self {
            Self::Boot | Self::Frunk => false,
            Self::Doors | Self::Windows => true,
        }
    }
}

impl Display for Door {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Frunk => write!(f, "frunk"),
            Self::Boot => write!(f, "boot"),
            Self::Doors => write!(f, "doors"),
            Self::Windows => write!(f, "windows"),
        }
    }
}

pub struct MonitorInputs {
    pub frunk: stateful::Receiver<DoorState>,
    pub boot: stateful::Receiver<DoorState>,
    pub doors: stateful::Receiver<DoorState>,
    pub windows: stateful::Receiver<DoorState>,
    pub user_present: stateful::Receiver<UserIsPresent>,
}

impl MonitorInputs {
    pub fn from_receivers(receivers: &Receivers) -> Self {
        Self {
            frunk: receivers.frunk.clone(),
            boot: receivers.boot.clone(),
            doors: receivers.doors.clone(),
            windows: receivers.windows.clone(),
            user_present: receivers.user_present.clone(),
        }
    }
}

pub fn monitor(
    car: &car::Config,
    receivers: MonitorInputs,
    reminders: &ReminderService,
    message_sink: stateless::Sender<Message>,
) {
    let id = car.id.clone();

    let (tx, rx) = stateful::create_pipe("tesla_doors");

    spawn(async move {
        let mut frunk_s = receivers.frunk.subscribe().await;
        let mut boot_s = receivers.boot.subscribe().await;
        let mut doors_s = receivers.doors.subscribe().await;
        let mut windows_s = receivers.windows.subscribe().await;
        let mut user_present_s = receivers.user_present.subscribe().await;

        let Ok(mut frunk) = frunk_s.recv().await else {
            error!(%id, "Failed to get initial user_present");
            return;
        };
        let Ok(mut boot) = boot_s.recv().await else {
            error!(%id, "Failed to get initial user_present");
            return;
        };
        let Ok(mut doors) = doors_s.recv().await else {
            error!(%id, "Failed to get initial user_present");
            return;
        };
        let Ok(mut windows) = windows_s.recv().await else {
            error!(%id, "Failed to get initial user_present");
            return;
        };
        let Ok(mut user_present) = user_present_s.recv().await else {
            error!(%id, "Failed to get initial user_present");
            return;
        };

        debug!(
            %id, "initial fo: {:?}, to: {:?}, do: {:?}, wo: {:?}, up: {:?}",
            frunk, boot, doors, windows, user_present
        );

        loop {
            select! {
                Ok(v) = frunk_s.recv() => frunk = v,
                Ok(v) = boot_s.recv() => boot = v,
                Ok(v) = doors_s.recv() => doors = v,
                Ok(v) = windows_s.recv() => windows = v,
                Ok(v) = user_present_s.recv() => user_present = v,
                else => break,
            };

            debug!(
                %id, "fo: {:?}, to: {:?}, do: {:?}, wo: {:?}, up: {:?}",
                frunk, boot, doors, windows, user_present
            );

            let mut open: Vec<Door> = vec![];
            if UserIsPresent::UserNotPresent == user_present {
                if DoorState::Open == frunk {
                    open.push(Door::Frunk);
                }

                if DoorState::Open == boot {
                    open.push(Door::Boot);
                }

                if DoorState::Open == doors {
                    open.push(Door::Doors);
                }

                if DoorState::Open == windows {
                    open.push(Door::Windows);
                }
            }

            debug!(%id, "open: {:?}", open);
            tx.try_send(open);
        }
    });

    // We only care if doors open for at least 120 seconds.
    let duration = Duration::from_mins(2);
    let rx = rx.delay_input(
        "tesla_doors (delayed)",
        duration,
        |(_, c)| !c.is_empty(),
        DelayInputOptions {
            skip_subsequent_delay: true,
        },
    );

    // Discard initial [] value and duplicate events.
    let rx = rx.filter(|(p, c)| p.is_some() || !c.is_empty());

    manage_reminder(car, rx, reminders, message_sink);
}

/// Manage the doors reminder for the delayed open-doors stream.
///
/// While doors are open, create a reminder and keep its message up to date
/// as the set of open doors changes; the reminder service re-sends the
/// message until the reminder is acknowledged, silenced or deleted. When
/// the doors are secured, delete the reminder and send a one-off message
/// confirming the car is secure.
fn manage_reminder(
    car: &car::Config,
    open_doors: stateful::Receiver<Vec<Door>>,
    reminders: &ReminderService,
    message_sink: stateless::Sender<Message>,
) {
    let key = format!("{}/doors", car.id);
    let car = car.clone();
    let reminders = reminders.clone();
    spawn(async move {
        let mut s = open_doors.subscribe().await;
        let mut reminder: Option<crate::reminders::Reminder> = None;

        while let Ok(open) = s.recv().await {
            debug!(%key, open = ?open, "Received open doors");
            if open.is_empty() {
                if reminder.take().is_some() {
                    debug!(%key, "Doors are secure; deleting reminder");
                    message_sink.try_send(doors_to_message(&car, &open));
                }
            } else {
                let message = doors_to_message(&car, &open);
                if let Some(r) = &reminder {
                    debug!(%key, "Open doors changed; updating reminder");
                    r.update_message(message);
                } else {
                    debug!(%key, "Doors are open; creating reminder");
                    reminder = Some(reminders.create(key.clone(), message));
                }
            }
        }
    });
}

fn doors_to_message(car: &car::Config, open: &[Door]) -> Message {
    let name = &car.name;

    let msg = match open {
        [] => format!("{name} is secure"),
        // The Tesla doors are open
        [doors] if doors.is_plural() => {
            format!("{name} {doors} are open")
        }
        // The Tesla frunk is open
        [door] if !door.is_plural() => {
            format!("{name} {door} is open")
        }
        // The Tesla frunk and boot are open
        // The Tesla frunk, boot and doors are open
        // The Tesla doors, boot and frunk are open
        [doors @ .., last] => {
            let doors = doors
                .iter()
                .map(Door::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name} {doors} and {last} are open")
        }
    };

    let priority = if open.is_empty() {
        MessagePriority::Info
    } else {
        MessagePriority::Error
    };

    new_message(msg, priority, &car.audience.doors)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::car::{AudienceConfig, MakeConfig};
    use robotica_common::robotica::entities::Id;

    fn test_car() -> car::Config {
        car::Config {
            id: Id::new("tesla-1").unwrap(),
            name: "Tesla".to_string(),
            oel_display: None,
            audience: AudienceConfig {
                errors: "brian".into(),
                locations: "brian".into(),
                doors: "brian".into(),
                charging: "brian".into(),
                private: "brian".into(),
            },
            arrival_radius_m: 0.0,
            exit_radius_m: 10.0,
            make: MakeConfig::Unknown,
        }
    }

    #[rstest::rstest]
    #[case(&[], "Tesla is secure", MessagePriority::Info)]
    #[case(&[Door::Frunk], "Tesla frunk is open", MessagePriority::Error)]
    #[case(&[Door::Boot], "Tesla boot is open", MessagePriority::Error)]
    #[case(&[Door::Doors], "Tesla doors are open", MessagePriority::Error)]
    #[case(&[Door::Windows], "Tesla windows are open", MessagePriority::Error)]
    #[case(&[Door::Frunk, Door::Boot], "Tesla frunk and boot are open", MessagePriority::Error)]
    #[case(
        &[Door::Frunk, Door::Boot, Door::Doors],
        "Tesla frunk, boot and doors are open",
        MessagePriority::Error
    )]
    fn test_doors_to_message(
        #[case] open: &[Door],
        #[case] body: &str,
        #[case] priority: MessagePriority,
    ) {
        let car = test_car();
        let message = doors_to_message(&car, open);
        assert_eq!(message.title, "Tesla");
        assert_eq!(message.body, body);
        assert_eq!(message.priority, priority);
        assert_eq!(message.audience.to_string(), "brian");
    }
}
