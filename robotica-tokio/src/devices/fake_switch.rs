//! Fake a digital on/off switch
use robotica_common::{
    mqtt::Json,
    robotica::{
        commands::Command,
        switch::{DeviceAction, DevicePower},
    },
};

use crate::pipes::{stateful, stateless};

/// Run a fake switch
#[must_use]
pub fn run(command_rx: stateless::Receiver<Json<Command>>) -> stateful::Receiver<DevicePower> {
    // let topic_substr: String = topic_substr.into();
    // let topic = format!("command/{topic_substr}");
    // let rx = subscription.subscribe_into_stateless::<Json<Command>>(&topic);
    let (tx, rx) = stateful::create_pipe("fake_switch");

    command_rx.for_each(move |Json(command)| match command {
        Command::Device(command) => {
            let status = match command.action {
                DeviceAction::TurnOff => DevicePower::Off,
                DeviceAction::TurnOn => DevicePower::On,
            };
            tx.try_send(status);
        }
        command => {
            tracing::error!("Invalid command, expected switch, got {:?}", command);
        }
    });

    rx
}
