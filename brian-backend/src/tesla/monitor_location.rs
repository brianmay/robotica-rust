use robotica_backend::{
    pipes::{stateful, stateless, Subscriber, Subscription},
    services::tesla::api::ChargingStateEnum,
    spawn,
};
use robotica_common::robotica::{
    audio::MessagePriority, locations::LocationList, message::Message,
};
use tap::Pipe;
use tokio::select;
use tracing::{debug, error};

use crate::{amber::car::ChargeRequest, car};

use super::{private::new_message, ChargingInformation, ShouldPlugin};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChargingMessage {
    Disconnected,
    Charging { limit: u8 },
    NoPower,
    Complete,
    Stopped,
}

impl ChargingMessage {
    const fn get(charging_info: &ChargingInformation) -> Self {
        let limit = charging_info.charge_limit;

        match charging_info.charging_state {
            ChargingStateEnum::Disconnected => Self::Disconnected,
            ChargingStateEnum::Charging | ChargingStateEnum::Starting => Self::Charging { limit },
            ChargingStateEnum::NoPower => Self::NoPower,
            ChargingStateEnum::Complete => Self::Complete,
            ChargingStateEnum::Stopped => Self::Stopped,
        }
    }

    fn to_string(self, level: u8) -> String {
        match self {
            Self::Disconnected => format!("is disconnected at {level}%"),
            Self::Charging { limit } => {
                format!("is charging from {level}% to {limit}%")
            }
            Self::NoPower => format!("plug failed at {level}%"),
            Self::Complete => format!("is finished charging at {level}%"),
            Self::Stopped => format!("has stopped charging at {level}%"),
        }
    }
}

fn announce_charging_state(
    car: &car::Config,
    old_charging_info: &ChargingInformation,
    charging_info: &ChargingInformation,
    message_sink: &stateless::Sender<Message>,
) {
    let name = &car.name;

    let plugged_in_msg = {
        let was_plugged_in = old_charging_info.charging_state.is_plugged_in();
        let is_plugged_in = charging_info.charging_state.is_plugged_in();

        if was_plugged_in && !is_plugged_in {
            Some("has been freed".to_string())
        } else if !was_plugged_in && is_plugged_in {
            Some("has been leashed".to_string())
        } else {
            None
        }
    };

    let charge_msg = {
        // We do not want an announcement every time the battery level changes.
        let level = charging_info.battery_level;
        // But we do want an announcement if other charging information changes.
        let old_msg = ChargingMessage::get(old_charging_info);
        let new_msg = ChargingMessage::get(charging_info);
        if old_msg == new_msg {
            None
        } else {
            new_msg.to_string(level).pipe(Some)
        }
    };

    if plugged_in_msg.is_some() || charge_msg.is_some() {
        let msg = [plugged_in_msg, charge_msg]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" and ");

        let msg = format!("{name} {msg}");
        let msg = new_message(msg, MessagePriority::DaytimeOnly, &car.audience.charging);
        message_sink.try_send(msg);
    }
}

pub fn monitor(
    car: &car::Config,
    message_sink: stateless::Sender<Message>,
    location_stream: stateful::Receiver<LocationList>,
    charging_info: stateful::Receiver<ChargingInformation>,
) -> stateful::Receiver<ShouldPlugin> {
    let (tx, rx) = stateful::create_pipe("tesla_should_plugin");

    let tesla = car.clone();

    spawn(async move {
        let mut location_s = location_stream.subscribe().await;
        let mut charging_info_s = charging_info.subscribe().await;
        let name = &tesla.name;

        let Ok(mut old_location) = location_s.recv().await else {
            error!("{name}: Failed to get initial Tesla location");
            return;
        };

        let Ok(mut old_charging_info) = charging_info_s.recv().await else {
            error!("{name}: Failed to get initial Tesla charging information");
            return;
        };

        debug!("{name}: Initial Tesla location: {:?}", old_location);
        debug!(
            "{name}: Initial Tesla charging information: {:?}",
            old_charging_info
        );

        loop {
            let should_plugin = if old_location.is_at_home()
                && !old_charging_info.charging_state.is_plugged_in()
                && old_charging_info.battery_level <= 80
            {
                ShouldPlugin::ShouldPlugin
            } else {
                ShouldPlugin::NoActionRequired
            };
            tx.try_send(should_plugin);

            select! {
                Ok(new_charging_info) = charging_info_s.recv() => {
                    if old_location.is_at_home()  {
                        announce_charging_state(&tesla, &old_charging_info, &new_charging_info, &message_sink);
                    }
                    old_charging_info = new_charging_info;
                },
                Ok(new_location) = location_s.recv() => {
                    if !old_location.is_near_home() && new_location.is_near_home() {
                        let level = old_charging_info.battery_level;

                        let (limit_type, limit) = match old_charging_info.charge_request_at_home {
                            ChargeRequest::ChargeTo(limit) => ("auto", limit),
                            ChargeRequest::Manual => ("manual", old_charging_info.charge_limit),
                        };
                        let msg = if level < limit {
                            format!("{name} is at {level}% and would {limit_type} charge to {limit}%")
                        } else {
                            format!("{name} is at {level}% and the {limit_type} limit is {limit}%")
                        };
                        let msg = new_message(msg, MessagePriority::DaytimeOnly, &tesla.audience.locations);
                        message_sink.try_send(msg);
                    }

                    old_location = new_location;
                }

            }
        }
    });

    rx
}
