use log::debug;
use serde::Deserialize;
use thiserror::Error;
use tokio::select;

use robotica_backend::{devices::hdmi::Command, entities, spawn};
use robotica_common::mqtt::{MqttMessage, QoS};

use crate::{robotica::Id, State};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
struct RoboticaCommand {
    input: u8,
    output: u8,
}

#[derive(Error, Debug)]
pub enum CommandErr {
    /// The Mark is invalid.
    #[error("Invalid mark {0}")]
    ParseError(#[from] serde_json::Error),

    /// UTF-8 error in Mark.
    #[error("Invalid UTF8")]
    Utf8Error(#[from] std::str::Utf8Error),
}

impl TryFrom<MqttMessage> for RoboticaCommand {
    type Error = CommandErr;

    fn try_from(msg: MqttMessage) -> Result<Self, Self::Error> {
        let mark: RoboticaCommand = serde_json::from_str(&msg.payload)?;
        Ok(mark)
    }
}

pub fn run(state: &mut State, location: &str, device: &str, addr: &str) {
    let id = Id::new(location, device);
    let topic = id.get_command_topic(&[]);

    let command_rx = state
        .subscriptions
        .subscribe_into_stateful::<RoboticaCommand>(&topic);

    let name = id.get_name("hdmi");
    let (tx, rx) = entities::create_stateless_entity(&name);

    spawn(async move {
        let mut rx_s = command_rx.subscribe().await;

        loop {
            select! {
                Ok((_, command)) = rx_s.recv() => {
                    let command = Command::SetInput(command.input, command.output);
                    tx.try_send(command);
                },
                else => break,
            };
        }
    });

    let mqtt = state.mqtt.clone();
    let addr = addr.to_string();
    let (rx, _) = robotica_backend::devices::hdmi::run(addr, rx, &Default::default());

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
    match value {
        Some(v) => v.to_string(),
        None => "HARD_OFF".to_string(),
    }
}
