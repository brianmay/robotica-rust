//! Dodgy HDMI matrix of unknown origin.
use std::fmt::Debug;

use bytes::Bytes;
use log::{debug, error};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, ToSocketAddrs},
    select, spawn,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::PIPE_SIZE;

/// A command to send to the HDMI matrix.
#[derive(Debug)]
pub enum Command {
    /// Set input for output.
    SetInput(u8, u8),
    /// Get and reset count of errors that have occurred.
    GetErrors(oneshot::Sender<u32>),
    /// Shutdown the server.
    Shutdown,
}

/// Options for HDMI matrix.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Should we disable the polling of the HDMI matrix?
    pub disable_polling: bool,
}

/// The state of the HDMI matrix.
///
/// # Errors
///
/// This function will return an error if the connection to the HDMI matrix fails.
///
/// # Panics
///
/// This function will panic if the connection to the HDMI matrix fails.
pub async fn run<A>(addr: A, options: &Options) -> (mpsc::Sender<Command>, JoinHandle<()>)
where
    A: ToSocketAddrs + Clone + Send + Sync + Debug + 'static,
{
    let (tx_cmd, mut rx_cmd) = mpsc::channel(PIPE_SIZE);

    let options = options.clone();
    let handle = spawn(async move {
        println!("client: Starting with addr {addr:?}");
        let mut errors = 0u32;
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(30));
        let addr = addr;

        loop {
            select! {
                _ = timer.tick() => {
                    if options.disable_polling  {
                        debug!("client: disabled polling {addr:?}");
                    } else {
                        debug!("client: polling {addr:?}");
                        poll(&addr).await.unwrap_or_else(|err| {
                            error!("client: Polling HDMI failed: {err}");
                            errors = errors.saturating_add(1);
                        });
                    }
                }

                Some(cmd) = rx_cmd.recv() => {
                    debug!("client: Received command {cmd:?} for {addr:?}");
                    match cmd {
                        Command::SetInput(input, output) => {
                            set_input(&addr, input, output).await.unwrap_or_else(|err| {
                                error!("client: Setting HDMI input failed: {err}");
                                errors = errors.saturating_add(1);
                            });
                        }
                        Command::GetErrors(tx) => {
                            let _ = tx.send(errors);
                            errors = 0;
                        }
                        Command::Shutdown => { break; },
                    };
                }
            }
        }
        println!("client: Ending");
    });

    (tx_cmd, handle)
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

async fn poll<A>(addr: A) -> Result<(), std::io::Error>
where
    A: ToSocketAddrs + Send + Sync + Debug,
{
    // Note channels are indexed from 1 not 0.
    let mut stream = connect(&addr).await?;
    for output in 1..=4 {
        let cmd = get_cmd_input_for_output(output);
        let response = send_command(&mut stream, &cmd).await?;
        let input = response[6];
        let output = response[4];
        let bytes: Bytes = response.into();
        debug!("Got HDMI response {} {} {bytes:#X}", input, output);
    }
    Ok(())
}

async fn set_input<A>(addr: A, input: u8, output: u8) -> Result<(), std::io::Error>
where
    A: ToSocketAddrs + Send + Sync + Debug,
{
    let mut stream = connect(&addr).await.unwrap();
    let cmd = get_cmd_switch(input, output);
    send_command(&mut stream, &cmd).await?;
    Ok(())
}

async fn send_command(stream: &mut TcpStream, bytes: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    debug!("Sending HDMI command {bytes:02X?}");
    stream.write_all(bytes).await?;

    let mut buffer = [0; 13];
    let _bytes = stream.read_exact(&mut buffer).await?;
    debug!("Receiving HDMI command {bytes:02X?}");

    if !check_checksum(buffer.as_ref()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Checksum error",
        ));
    }

    Ok(buffer.to_vec())
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
