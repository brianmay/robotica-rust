use std::fmt::Display;

use robotica_backend::{
    pipes::{delays::DelayInputOptions, stateful, stateless, Subscriber, Subscription},
    spawn,
};
use robotica_common::{
    mqtt::{BoolError, MqttMessage},
    robotica::{audio::MessagePriority, message::Message},
};
use std::time::Duration;
use thiserror::Error;
use tokio::select;
use tracing::debug;

use crate::tesla::private::new_message;

use super::{Config, Receivers};

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

#[must_use]
pub fn monitor(tesla: &Config, receivers: MonitorInputs) -> stateless::Receiver<Message> {
    let (message_tx, message_rx) = stateless::create_pipe("tesla_doors_message");

    let (tx, rx) = stateful::create_pipe("tesla_doors");

    let tesla_clone = tesla.clone();
    spawn(async move {
        let mut frunk_s = receivers.frunk.subscribe().await;
        let mut boot_s = receivers.boot.subscribe().await;
        let mut doors_s = receivers.doors.subscribe().await;
        let mut windows_s = receivers.windows.subscribe().await;
        let mut user_present_s = receivers.user_present.subscribe().await;
        let name = &tesla_clone.name;

        loop {
            select! {
                Ok(_) = frunk_s.recv() => {},
                Ok(_) = boot_s.recv() => {},
                Ok(_) = doors_s.recv() => {},
                Ok(_) = windows_s.recv() => {},
                Ok(_) = user_present_s.recv() => {},
                else => break,
            };

            let mut open: Vec<Door> = vec![];

            let maybe_user_present = receivers.user_present.get().await;
            if Some(UserIsPresent::UserNotPresent) == maybe_user_present {
                let maybe_frunk = receivers.frunk.get().await;
                let maybe_boot = receivers.boot.get().await;
                let maybe_doors = receivers.doors.get().await;
                let maybe_windows = receivers.windows.get().await;

                debug!(
                    "{name}: fo: {:?}, to: {:?}, do: {:?}, wo: {:?}, up: {:?}",
                    maybe_frunk, maybe_boot, maybe_doors, maybe_windows, maybe_user_present
                );

                if Some(DoorState::Open) == maybe_frunk {
                    open.push(Door::Frunk);
                }

                if Some(DoorState::Open) == maybe_boot {
                    open.push(Door::Boot);
                }

                if Some(DoorState::Open) == maybe_doors {
                    open.push(Door::Doors);
                }

                // Ignore windows for now, as Tesla often reporting these are open when they are not.
                // if let Some(TeslaDoorState::Open) = maybe_wo {
                //     open.push(Door::Windows)
                // }
            } else {
                debug!("{name}: up: {:?}", maybe_user_present);
            }

            debug!("{name}: open: {:?}", open);
            tx.try_send(open);
        }
    });

    // We only care if doors open for at least 120 seconds.
    let duration = Duration::from_secs(120);
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

    // Repeat the last value every 5 minutes.
    let duration = Duration::from_secs(300);
    let rx = rx.delay_repeat("tesla_doors (repeat)", duration, |(_, c)| !c.is_empty());

    // Output the message.
    let tesla = tesla.clone();
    spawn(async move {
        let mut s = rx.subscribe().await;
        while let Ok(open) = s.recv().await {
            debug!("open received: {:?}", open);
            let msg = doors_to_message(&tesla, &open);
            let msg = new_message(msg, MessagePriority::Urgent, &tesla.audience.doors);
            message_tx.try_send(msg);
        }
    });

    message_rx
}

fn doors_to_message(tesla: &Config, open: &[Door]) -> String {
    let name = &tesla.name;

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
    msg
}
