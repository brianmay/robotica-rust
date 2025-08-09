use robotica_tokio::pipes::{Subscriber, Subscription};
use tokio::select;
use tracing::debug;

use robotica_common::mqtt::{Json, MqttMessage, QoS, Retain};
use robotica_common::robotica::commands;
use robotica_tokio::{devices::hdmi::Command, pipes::stateless, spawn};

use crate::{robotica::Id, InitState};

pub fn run(state: &mut InitState, location: &str, device: &str, addr: &str) {
    let id = Id::new(location, device);
    let topic = id.get_command_topic(&[]);

    let command_rx = state
        .subscriptions
        .subscribe_into_stateless::<Json<commands::Command>>(&topic);

    let name = id.get_name("hdmi");
    let (tx, rx) = stateless::create_pipe(name);

    spawn(async move {
        let mut rx_s = command_rx.subscribe().await;

        loop {
            select! {
                Ok(Json(command)) = rx_s.recv() => {
                    if let commands::Command::Hdmi(command) = command {
                        let command = Command::SetInput(command.input, command.output);
                        tx.try_send(command);
                    } else {
                        debug!("Invalid command: {:?}", command);
                    }
                },
                else => break,
            };
        }
    });

    let mqtt = state.mqtt.clone();
    let addr = addr.to_string();
    let (rx, _) = robotica_tokio::devices::hdmi::run(
        addr,
        rx,
        &robotica_tokio::devices::hdmi::Options::default(),
    );

    spawn(async move {
        let mut rx_s = rx.subscribe().await;

        loop {
            select! {
                Ok(status) = rx_s.recv() => {
                    debug!("HDMI {status:?}");
                    let status = status.unwrap_or_default();

                    let iter = status.iter().map(|x| map_input(*x));
                    for (output, input) in iter.enumerate() {
                        // Arrays are 0 based, but outputs are 1 based.
                        let output = format!("output{}", output + 1);
                        let topic = id.get_state_topic(&output.to_string());
                        let payload = input;
                        let message = MqttMessage::new(topic, payload, Retain::Retain, QoS::AtLeastOnce);
                        mqtt.try_send(message);
                    }
                },
                else => break,
            };
        }
    });
}

fn map_input(value: Option<u8>) -> String {
    value.map_or_else(|| "HARD_OFF".to_string(), |v| v.to_string())
}
