//! Dodgy HDMI matrix of unknown origin.
use std::fmt::Debug;

use log::{debug, info};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, ToSocketAddrs},
    select, spawn,
    task::JoinHandle,
};

use crate::{
    entities::{self, StatefulData},
    is_debug_mode,
};

/// A command to send to the HDMI matrix.
#[derive(Clone, Debug)]
pub enum Command {
    /// Set input for output.
    SetInput(u8, u8),
    /// Shutdown the server.
    Shutdown,
}

/// Options for HDMI matrix.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Should we disable the polling of the HDMI matrix?
    pub disable_polling: bool,
}

type Status = [Option<u8>; 4];

/// A connection to an HDMI matrix failed.
#[derive(Error, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// An IO Error occurred.
    #[error("IO Error {0}")]
    IoError(String),
}

/// The state of the HDMI matrix.
///
/// # Errors
///
/// This function will return an error if the connection to the HDMI matrix fails.
pub fn run<A>(
    addr: A,
    rx_cmd: entities::Receiver<Command>,
    options: &Options,
) -> (
    entities::Receiver<StatefulData<Result<Status, Error>>>,
    JoinHandle<()>,
)
where
    A: ToSocketAddrs + Clone + Send + Sync + Debug + 'static,
{
    let options = options.clone();
    let name = format!("{addr:?}");
    let (tx, rx) = entities::create_stateful_entity(name);

    let handle = spawn(async move {
        debug!("hdmi: Starting with addr {addr:?}");
        let mut rx_cmd_s = rx_cmd.subscribe().await;
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(30));
        let addr = addr;

        let mut status: Status = [None; 4];

        loop {
            select! {
                _ = timer.tick() => {
                    if options.disable_polling  {
                        debug!("hdmi: disabled polling {addr:?}");
                    } else {
                        match poll(&addr).await {
                            Ok(new_status) => {
                                status = new_status;
                                tx.try_send(Ok(status));
                            },
                            Err(e) => {
                                debug!("hdmi: error polling {addr:?}: {e}");
                                tx.try_send(Err(Error::IoError(e.to_string())));
                            }
                        }
                    }
                }

                Ok(cmd) = rx_cmd_s.recv() => {
                    debug!("hdmi: Received command {cmd:?} for {addr:?}");
                    match cmd {
                        Command::SetInput(input, output) => {
                            match set_input(&addr, input, output, &status).await {
                                Ok(new_status) => {
                                    status = new_status;
                                    tx.try_send(Ok(status));
                                },
                                Err(e) => {
                                    info!("hdmi: error setting input {addr:?}: {e}");
                                    tx.try_send(Err(Error::IoError(e.to_string())));
                                }
                            }
                        }
                        Command::Shutdown => { break; },
                    };
                }
            }
        }
        debug!("hdmi: Ending");
    });

    (rx, handle)
}

async fn connect<A>(addr: A) -> Result<TcpStream, std::io::Error>
where
    A: ToSocketAddrs + Send + Sync + Debug,
{
    let duration = std::time::Duration::from_secs(5);
    let future = TcpStream::connect(addr);
    match tokio::time::timeout(duration, future).await {
        Ok(Ok(stream)) => Ok(stream),
        Ok(Err(err)) => Err(err),
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "Connection timed out",
        )),
    }
}

async fn poll<A>(addr: A) -> Result<Status, std::io::Error>
where
    A: ToSocketAddrs + Send + Sync + Debug,
{
    let mut status = [None; 4];

    if is_debug_mode() {
        debug!("hdmi: Not polling in debug mode");
        return Ok(status);
    }

    debug!("hdmi: polling {addr:?}");

    // Note channels are indexed from 1 not 0.
    let mut stream = connect(&addr).await?;
    for output in 1..=4 {
        let cmd = get_cmd_input_for_output(output);
        let response = send_command(&mut stream, &cmd).await?;
        let input = response[6];
        let output = response[4];
        if let Some(v) = status.get_mut((output - 1) as usize) {
            *v = Some(input);
        }
        debug!("hdmi: Got HDMI response {} {}", input, output);
    }
    Ok(status)
}

async fn set_input<A>(
    addr: A,
    input: u8,
    output: u8,
    status: &Status,
) -> Result<Status, std::io::Error>
where
    A: ToSocketAddrs + Send + Sync + Debug,
{
    if is_debug_mode() {
        debug!("hdmi: Not setting input in debug mode");
        return Ok(*status);
    }

    let mut status = *status;
    let mut stream = connect(&addr).await?;
    let cmd = get_cmd_switch(input, output);
    let response = send_command(&mut stream, &cmd).await?;
    // Note response doesn't include the newly selected input.
    let output = response[6];
    if let Some(v) = status.get_mut((output - 1) as usize) {
        *v = Some(input);
    }
    debug!("hdmi: Got HDMI response {} {}", input, output);
    Ok(status)
}

async fn send_command(
    stream: &mut TcpStream,
    out_bytes: &[u8; 13],
) -> Result<[u8; 13], std::io::Error> {
    let duration = std::time::Duration::from_secs(5);

    let result = tokio::time::timeout(duration, async {
        debug!("hdmi: Sending HDMI command {out_bytes:02X?}");
        stream.write_all(out_bytes).await?;

        let mut in_bytes = [0; 13];
        let _bytes = stream.read_exact(&mut in_bytes).await?;
        debug!("hdmi: Receiving HDMI response {in_bytes:02X?}");

        Ok(in_bytes)
    })
    .await;

    let in_bytes = result.map_or_else(
        |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Data timed out",
            ))
        },
        |in_bytes| in_bytes,
    )?;

    if !check_checksum(in_bytes.as_ref()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Checksum error",
        ));
    }

    Ok(in_bytes)
}

#[must_use]
fn calc_checksum(bytes: &[u8]) -> u8 {
    let mut sum = 0u16;
    for byte in bytes {
        sum = sum.wrapping_add(u16::from(*byte));
    }
    #[allow(clippy::cast_possible_truncation)]
    if bytes.len() > 13 {
        (sum & 0xff) as u8
    } else {
        (0x100 - (sum & 0xff)) as u8
    }
}

fn add_checksum(bytes: &mut [u8]) {
    let last = bytes.len() - 1;
    let checksum = calc_checksum(&bytes[0..last]);
    bytes[last] = checksum;
}

fn check_checksum(bytes: &[u8]) -> bool {
    let last = bytes.len() - 1;
    let checksum = calc_checksum(&bytes[0..last]);
    checksum == bytes[last]
}

fn get_cmd_switch(input: u8, output: u8) -> [u8; 13] {
    let mut bytes = [
        0xa5, 0x5b, 0x02, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff,
    ];
    bytes[4] = input;
    bytes[6] = output;
    add_checksum(&mut bytes);
    bytes
}

fn get_cmd_input_for_output(output: u8) -> [u8; 13] {
    let mut bytes = [
        0xa5, 0x5b, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff,
    ];
    bytes[4] = output;
    add_checksum(&mut bytes);
    bytes
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_calc_checksum() {
        let bytes = [
            0xa5, 0x5b, 0x02, 0x03, 0x03, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let checksum = calc_checksum(&bytes);
        assert_eq!(checksum, 0xf4);
    }

    #[test]
    fn test_add_checksum() {
        let mut bytes = [
            0xa5, 0x5b, 0x02, 0x03, 0x03, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x99,
        ];
        let expected_bytes = [
            0xa5, 0x5b, 0x02, 0x03, 0x03, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF4,
        ];
        add_checksum(&mut bytes);
        assert_eq!(bytes, expected_bytes);
    }

    #[test]
    fn test_check_checksum() {
        let bytes = [
            0xa5, 0x5b, 0x02, 0x03, 0x03, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF4,
        ];
        assert!(check_checksum(&bytes));
    }

    #[test]
    fn test_get_cmd_switch() {
        let bytes = get_cmd_switch(3, 4);
        let expected_bytes = [
            0xa5, 0x5b, 0x02, 0x03, 0x03, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF4,
        ];
        assert_eq!(bytes, expected_bytes);
    }

    #[test]
    fn test_get_cmd_input_for_output() {
        let bytes = get_cmd_input_for_output(4);
        let expected_bytes = [
            0xa5, 0x5b, 0x02, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF9,
        ];
        assert_eq!(bytes, expected_bytes);
    }
}
