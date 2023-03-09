use thiserror::Error;
use tokio::select;
use tracing::debug;

use robotica_backend::{devices::hdmi::Command, entities, spawn};
use robotica_common::mqtt::{Json, MqttMessage, QoS};
use robotica_common::robotica::commands;

use crate::{robotica::Id, State};

#[derive(Error, Debug)]
pub enum CommandErr {
    /// The Mark is invalid.
    #[error("Invalid mark {0}")]
    ParseError(#[from] serde_json::Error),

    /// UTF-8 error in Mark.
    #[error("Invalid UTF8")]
    Utf8Error(#[from] std::str::Utf8Error),
}

pub fn run(state: &mut State, location: &str, device: &str, addr: &str) {
    let id = Id::new(location, device);
    let topic = id.get_command_topic(&[]);

    let command_rx = state
        .subscriptions
        .subscribe_into_stateless::<Json<commands::Command>>(&topic);

    let name = id.get_name("hdmi");
    let (tx, rx) = entities::create_stateless_entity(name);

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
    let (rx, _) = robotica_backend::devices::hdmi::run(
        addr,
        rx,
        &robotica_backend::devices::hdmi::Options::default(),
    );

    spawn(async move {
        let mut rx_s = rx.subscribe().await;

        loop {
            select! {
                Ok((_, status)) = rx_s.recv() => {
                    debug!("HDMI {status:?}");
                    let status = match status {
                        Ok(values) => values,
                        Err(_err) => [None; 4],
                    };

                    let iter = status.iter().map(|x| map_input(*x));
                    for (output, input) in iter.enumerate() {
                        // Arrays are 0 based, but outputs are 1 based.
                        let output = format!("output{}", output + 1);
                        let topic = id.get_state_topic(&output.to_string());
                        let payload = input;
                        let message = MqttMessage::new(topic, payload, true, QoS::AtLeastOnce);
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
