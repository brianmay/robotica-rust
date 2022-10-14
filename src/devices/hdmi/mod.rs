//! Dodgy HDMI matrix of unknown origin.
use log::{debug, error};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    select, spawn,
};

use crate::entities::{create_stateless_entity, Sender};

/// A command to send to the HDMI matrix.
#[derive(Clone, Debug)]
pub enum Command {
    /// Set input for output.
    SetInput(u8, u8),
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
pub async fn test() -> Sender<Command> {
    let (tx_cmd, rx_cmd) = create_stateless_entity("test");
    // let (tx_resp, mut rx_resp) = entities::create_entity::<[Status; 4]>("hdmi");

    spawn(async move {
        let mut rx_cmd = rx_cmd.subscribe().await;

        let mut timer = tokio::time::interval(std::time::Duration::from_secs(30));
        // let cmd = get_cmd_input_for_output(4);

        let addr = "hdmi.pri:8000";
        let mut stream = TcpStream::connect(addr).await.unwrap();

        // stream.write_all(&cmd).await.unwrap();

        // let mut buffer = BytesMut::with_capacity(13);

        loop {
            select! { // 3
                // result = stream.read_buf(&mut buffer) => {
                //     match result {
                //         Ok(size) if size >= 13 => {
                //             let response = buffer.split_to(size);
                //             println!("Response: {:#X?}", response);
                //             println!("Checksum: {}", check_checksum(&response));
                //             if response[3] == 0x01 {
                //                 let input = response[6];
                //                 let output = response[4];
                //                 println!("Answer: {} {}", input, output);
                //             }
                //             buffer.clear();
                //         },
                //         Ok(_) => {
                //             println!("Read: {:#X?}", buffer);
                //         },
                //         Err(err) => {
                //             if err.kind() == std::io::ErrorKind::UnexpectedEof {
                //                 println!("Connection closed");
                //                 break;
                //             }
                //             println!("Error: {}", err);
                //         }
                //     }

                // }
                _ = timer.tick() => {
                    poll(&mut stream).await.unwrap_or_else(|err| {
                        error!("Polling HDMI failed: {err}");
                    });
                }

                Ok(cmd) = rx_cmd.recv() => {
                    println!("Received command: {:?}", cmd);
                    let cmd = match cmd {
                        Command::SetInput(input, output) => get_cmd_switch(input, output),
                    };
                    send_command(&mut stream, &cmd).await.unwrap_or_else(|err| {
                        error!("Sending HDMI command failed: {err}");
                        vec![]
                    });
                }
            }
        }
    });

    tx_cmd
}

async fn poll(stream: &mut TcpStream) -> Result<(), std::io::Error> {
    // Note channels are indexed from 1 not 0.
    for output in 1..=4 {
        debug!("Sending HDMI query command");
        let cmd = get_cmd_input_for_output(output);
        let response = send_command(stream, &cmd).await?;
        let input = response[6];
        let output = response[4];
        debug!("Got HDMI response {} {}", input, output);
    }
    Ok(())
}

async fn send_command(stream: &mut TcpStream, bytes: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    stream.write_all(bytes).await?;

    let mut buffer = [0; 13];
    let _bytes = stream.read_exact(&mut buffer).await?;

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
