use std::time::Duration;

use chrono::{DateTime, Timelike, Utc};
use robotica_common::robotica::{message::Message, message::MessagePriority, zones::OccupiedZones};
use robotica_tokio::{
    pipes::{stateful, stateless, Subscriber, Subscription},
    services::tesla::api::ChargingStateEnum,
    spawn,
};
use tap::Pipe;
use tokio::select;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{debug, error};

use crate::{amber::car::ChargeRequest, amber::Prices, car, tesla::AutoCharge};

use super::{private::new_message, ChargingInformation, PluginAction};

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
        let msg = new_message(msg, MessagePriority::Info, &car.audience.charging);
        message_sink.try_send(msg);
    }
}

/// On manual charge, decide whether to alert the user to switch back to
/// automatic charging.
///
/// Alerts when the car at home is plugged in and:
///
/// - it is charging at an expensive price (auto mode would not be charging
///   now, so the manual charge override is costing money), or
/// - it is not charging although charging is needed now or later tonight
///   (auto mode would charge it, so the manual charge override is missing a
///   cheap charge).
///
/// An unplugged car is never alerted: auto mode cannot charge it either, and
/// the plugin action covers plugging in.
const fn should_switch_to_auto(
    charging_info: &ChargingInformation,
    is_at_home: bool,
    expensive: bool,
    would_charge_at_home: bool,
) -> bool {
    let on_manual_charge = matches!(charging_info.auto_charge, AutoCharge::Manual);
    let is_plugged_in = charging_info.charging_state.is_plugged_in();
    let is_charging = charging_info.charging_state.is_charging();

    on_manual_charge
        && is_at_home
        && is_plugged_in
        && if is_charging {
            expensive
        } else {
            would_charge_at_home
        }
}

/// Whether the current general price is expensive enough that charging on
/// manual charge should be alerted on, at the given time.
fn is_expensive(prices: &Prices, threshold: f32, now: DateTime<Utc>) -> bool {
    prices
        .general()
        .current(&now)
        .is_some_and(|price| price.effective_per_kwh() > threshold)
}

/// Produce a stream of whether the current general price is expensive enough
/// that charging on manual charge should be alerted on.
///
/// Recomputed when amber publishes new prices and when the amber pricing
/// interval tick fires, so a price interval boundary is picked up immediately
/// instead of at the next price refresh. A missing current price interval
/// counts as not expensive.
pub fn expensive_price(
    prices: stateful::Receiver<std::sync::Arc<Prices>>,
    tick: stateless::Receiver<DateTime<Utc>>,
    threshold: f32,
) -> stateful::Receiver<bool> {
    let (tx, rx) = stateful::create_pipe("tesla_expensive_price");

    spawn(async move {
        let mut prices_s = prices.subscribe().await;
        let mut tick_s = tick.subscribe().await;

        let Ok(mut prices) = prices_s.recv().await else {
            error!("tesla_expensive_price: failed to get initial prices");
            return;
        };
        tx.try_send(is_expensive(&prices, threshold, Utc::now()));

        loop {
            select! {
                Ok(new_prices) = prices_s.recv() => {
                    prices = new_prices;
                    tx.try_send(is_expensive(&prices, threshold, Utc::now()));
                }
                Ok(boundary) = tick_s.recv() => {
                    // The interval boundary tick is the moment the current
                    // price interval changes; re-evaluate the latest prices
                    // at it.
                    tx.try_send(is_expensive(&prices, threshold, boundary));
                }
            }
        }
    });

    rx
}

pub fn monitor(
    car: &car::Config,
    message_sink: stateless::Sender<Message>,
    location_stream: stateful::Receiver<OccupiedZones>,
    charge_request: stateful::Receiver<ChargeRequest>,
    expensive: stateful::Receiver<bool>,
    charging_info: stateful::Receiver<ChargingInformation>,
) -> stateful::Receiver<PluginAction> {
    let (tx, rx) = stateful::create_pipe("tesla_should_plugin");

    let car = car.clone();

    spawn(async move {
        let id = &car.id;
        let name = &car.name;

        let mut location_s = location_stream.subscribe().await;
        let mut charge_request_s = charge_request.subscribe().await;
        let mut expensive_s = expensive.subscribe().await;
        let mut charging_info_s = charging_info.subscribe().await;

        let Ok(mut location) = location_s.recv().await else {
            error!(%id, "Failed to get initial Tesla location");
            return;
        };

        let Ok(mut charge_request) = charge_request_s.recv().await else {
            error!(%id, "Failed to get initial charge request information");
            return;
        };

        let Ok(mut expensive) = expensive_s.recv().await else {
            error!(%id, "Failed to get initial price information");
            return;
        };

        let Ok(mut charging_info) = charging_info_s.recv().await else {
            error!(%id, "Failed to get initial Tesla charging information");
            return;
        };

        debug!(%id, "Initial Tesla location: {:?}", location);
        debug!(
            %id,
            "Initial Tesla charging information: {:?}",
            charging_info
        );

        // The in_window check in the loop body depends on the current time,
        // but the select! below only wakes when the location, charge request
        // or charging information changes. This timer is required so that
        // in_window is re-evaluated as time passes; without it the 18:00-22:00
        // window would never be entered or left while the car is asleep and no
        // charge plan is active.
        let mut window_ticker = interval(Duration::from_mins(5));
        window_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // The first tick completes immediately; consume it so the timer does
        // not fire an extra iteration before time has actually passed.
        window_ticker.tick().await;

        loop {
            let time = chrono::Local::now();
            let in_window = (18..=22).contains(&time.hour());

            let would_charge_at_home = matches!(charge_request, ChargeRequest::ChargeTo(limit) if limit > charging_info.battery_level)
                || (in_window && charging_info.battery_level < 70);

            let is_at_home = location.is_at_home();

            tx.try_send(PluginAction {
                switch_to_auto: should_switch_to_auto(
                    &charging_info,
                    is_at_home,
                    expensive,
                    would_charge_at_home,
                ),
                plugin: is_at_home
                    && !charging_info.charging_state.is_plugged_in()
                    && would_charge_at_home,
            });

            // This is applicable on assumption that car will arrive at home.
            let announce_action = PluginAction {
                switch_to_auto: matches!(charging_info.auto_charge, AutoCharge::Manual),
                plugin: would_charge_at_home,
            };

            select! {
                Ok(new_charging_info) = charging_info_s.recv() => {
                    announce_charging_state(&car, &charging_info, &new_charging_info, &message_sink);
                    charging_info = new_charging_info;
                },
                Ok(new_charge_request) = charge_request_s.recv() => {
                    charge_request = new_charge_request;
                }
                Ok(new_expensive) = expensive_s.recv() => {
                    expensive = new_expensive;
                }
                Ok(new_location) = location_s.recv() => {
                    if !location.is_near_home() && new_location.is_near_home() {
                        let level = charging_info.battery_level;
                        let mut postfix = String::new();
                        if announce_action.switch_to_auto {
                            postfix += " and is on manual charge";
                        }
                        if announce_action.plugin {
                            postfix += " and should be plugged in";
                        }
                        let msg = format!("{name} is at {level}%{postfix}");
                        let msg = new_message(msg, MessagePriority::Info, &car.audience.locations);
                        message_sink.try_send(msg);
                    }
                    location = new_location;
                }
                _ = window_ticker.tick() => {
                    // No state to update here; time may have crossed a window
                    // boundary, so the loop body re-evaluates in_window with
                    // the current time.
                }
            }
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use robotica_tokio::services::tesla::api::ChargingStateEnum;

    use super::super::{AutoCharge, ChargingInformation};
    use super::*;

    fn charging_info(
        charging_state: ChargingStateEnum,
        auto_charge: AutoCharge,
    ) -> ChargingInformation {
        ChargingInformation {
            battery_level: 60,
            charge_limit: 80,
            auto_charge,
            charging_state,
        }
    }

    // Charging, manual charge, at home, expensive: alert.
    #[rstest::rstest]
    #[case(
        ChargingStateEnum::Charging,
        AutoCharge::Manual,
        true,
        true,
        false,
        true
    )]
    // Charging, manual charge, at home, cheap: charging deliberately at a
    // cheap price; would_charge_at_home must not trigger the alert.
    #[case(
        ChargingStateEnum::Charging,
        AutoCharge::Manual,
        true,
        false,
        true,
        false
    )]
    // Charging, manual charge, away from home: auto mode does nothing away
    // from home either.
    #[case(
        ChargingStateEnum::Charging,
        AutoCharge::Manual,
        false,
        true,
        false,
        false
    )]
    // Charging, automatic charge: nothing to switch to.
    #[case(
        ChargingStateEnum::Charging,
        AutoCharge::Auto,
        true,
        true,
        false,
        false
    )]
    // Plugged in, not charging, manual charge, at home, charging needed now
    // or later tonight: alert on the missed charge.
    #[case(
        ChargingStateEnum::Stopped,
        AutoCharge::Manual,
        true,
        false,
        true,
        true
    )]
    // Plugged in, not charging, manual charge, at home, no charging needed.
    #[case(
        ChargingStateEnum::Stopped,
        AutoCharge::Manual,
        true,
        false,
        false,
        false
    )]
    // Unplugged: auto mode cannot charge it either, the plugin action covers
    // plugging in.
    #[case(
        ChargingStateEnum::Disconnected,
        AutoCharge::Manual,
        true,
        false,
        true,
        false
    )]
    fn test_should_switch_to_auto(
        #[case] charging_state: ChargingStateEnum,
        #[case] auto_charge: AutoCharge,
        #[case] is_at_home: bool,
        #[case] expensive: bool,
        #[case] would_charge_at_home: bool,
        #[case] expected: bool,
    ) {
        let charging_info = charging_info(charging_state, auto_charge);
        assert_eq!(
            should_switch_to_auto(&charging_info, is_at_home, expensive, would_charge_at_home),
            expected
        );
    }
}
