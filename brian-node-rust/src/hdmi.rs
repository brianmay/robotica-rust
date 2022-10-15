use robotica_node_rust::{
    devices::hdmi::Command,
    sources::mqtt::{Message, QoS},
    spawn, PIPE_SIZE,
};
use serde::Deserialize;
use thiserror::Error;
use tokio::{select, sync::mpsc};

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

impl TryFrom<Message> for RoboticaCommand {
    type Error = CommandErr;

    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        let payload: String = msg.try_into()?;
        let mark: RoboticaCommand = serde_json::from_str(&payload)?;
        Ok(mark)
    }
}

pub fn run(state: &mut State, location: &str, device: &str, addr: &str) {
    let id = Id::new(location, device);
    let topic = id.get_command_topic(&[]);

    let command_rx = state
        .subscriptions
        .subscribe_into_stateful::<RoboticaCommand>(&topic);

    let (tx, rx) = mpsc::channel(PIPE_SIZE);

    spawn(async move {
        let mut rx_s = command_rx.subscribe().await;

        loop {
            select! {
                Ok((_, command)) = rx_s.recv() => {
                    let command = Command::SetInput(command.input, command.output);
                    tx.send(command).await.unwrap();
                },
                else => break,
            };
        }
    });

    let mqtt_out = state.mqtt_out.clone();
    let addr = addr.to_string();
    let (rx, _) = robotica_node_rust::devices::hdmi::run(addr, rx, &Default::default());

    spawn(async move {
        let mut rx_s = rx.subscribe().await;

        loop {
            select! {
                Ok((_, status)) = rx_s.recv() => {
                    println!("HDMI {status:?}");
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
                        let message = Message::from_string(&topic, &payload, false, QoS::at_least_once());
                        mqtt_out.send(message);
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
