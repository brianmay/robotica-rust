//! Discover and control LIFX devices

use std::{net::SocketAddr, time::Duration};

use lifx_core::{BuildOptions, Message, RawMessage};
use robotica_common::robotica::lights::{Colors, PowerColor, PowerLevel, State, HSBK};
use thiserror::Error;
use tokio::{
    net::UdpSocket,
    select,
    time::{interval, sleep_until, Instant},
};
use tracing::{debug, error, info};

use crate::{
    entities::{self},
    spawn,
};

/// Configuration for the LIFX discovery process
pub struct DiscoverConfig {
    /// The broadcast address to use for discovery.
    pub broadcast: String,

    /// The time between discovery requests.
    pub poll_time: Duration,

    /// The time before the light is considered unreachable.
    pub device_timeout: Duration,

    /// The time before each API call times out.
    pub api_timeout: Duration,

    /// The number of times to retry an API call.
    pub num_retries: u8,
}

/// An error discovering LIFX devices
#[derive(Debug, Error)]
pub enum DiscoverError {
    /// A UDP socket error occurred.
    #[error("udp error: {0}")]
    Udp(#[from] tokio::io::Error),
}

/// Discover LIFX devices on the network
///
/// # Errors
///
/// Returns an error if the UDP socket cannot be created.
pub async fn discover(config: DiscoverConfig) -> Result<entities::Receiver<Device>, DiscoverError> {
    let (tx, rx) = entities::create_stateless_entity("lifx");

    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;

    spawn(async move {
        let mut interval = interval(config.poll_time);
        let mut buf = [0; 1024];

        loop {
            select! {
                _ = interval.tick() => {
                    debug!("Sending GetService");
                    let msg = Message::GetService;
                    send_broadcast(&socket, msg, &config.broadcast).await.unwrap_or_else(|e| {
                        error!("Error sending GetService: {e:?}");
                    });
                }

                Ok((len, addr)) = socket.recv_from(&mut buf) => {
                    debug!("Discover received {len} bytes from {addr}");
                    match RawMessage::unpack(&buf[..len]) {
                        Ok(raw) => {
                            let target = raw.frame_addr.target;
                            let device_timeout = config.device_timeout;
                            let api_timeout = config.api_timeout;
                            let num_retries = config.num_retries;
                            let device = Device { target, addr, device_timeout, api_timeout, num_retries };
                            tx.try_send(device);
                        }

                        Err(e) => {
                            error!("Error unpacking message: {e:?}");
                        }
                    }
                }
            }
        }
    });

    Ok(rx)
}

fn hsbk_to_lifx(hsbk: HSBK) -> lifx_core::HSBK {
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    lifx_core::HSBK {
        hue: (hsbk.hue * 65535.0 / 360.0) as u16,
        saturation: (hsbk.saturation * 65535.0 / 100.0) as u16,
        brightness: (hsbk.brightness * 65535.0 / 100.0) as u16,
        kelvin: hsbk.kelvin,
    }
}

/// A LIFX device
#[derive(Clone, Debug)]
pub struct Device {
    target: u64,
    addr: SocketAddr,
    device_timeout: Duration,
    api_timeout: Duration,
    num_retries: u8,
}

#[derive(Debug)]
enum DeviceState {
    Online(Device, Instant, u8),
    Offline,
}

/// An error talking to a LIFX device
#[derive(Debug, Error)]
pub enum LifxError {
    /// The device is offline.
    #[error("timed out")]
    Timeout,

    /// A LIFX error occurred.
    #[error("lifx error: {0}")]
    Lifx(#[from] lifx_core::Error),

    /// A UDP socket error occurred.
    #[error("udp error: {0}")]
    Udp(#[from] tokio::io::Error),

    /// A bad response was received.
    #[error("bad response")]
    BadResponse,

    /// The device is offline.
    #[error("Device is offline")]
    DeviceOffline,
}

impl DeviceState {
    fn set_online(&mut self, device: Device) {
        let seq = match self {
            DeviceState::Online(_, _, seq) => *seq,
            DeviceState::Offline => 0,
        };
        let timeout_time = device.device_timeout;
        *self = DeviceState::Online(device, Instant::now() + timeout_time, seq);
    }

    fn renew_online(&mut self) {
        if let DeviceState::Online(device, expires, _) = self {
            *expires = Instant::now() + device.device_timeout;
        }
    }

    fn set_offline(&mut self) {
        *self = DeviceState::Offline;
    }

    async fn set_power_color(
        &mut self,
        power_color: &PowerColor,
        config: &DeviceConfig,
    ) -> Result<(), LifxError> {
        if let DeviceState::Online(device, _, seq) = self {
            let socket = UdpSocket::bind("0.0.0.0:0").await?;
            // let power = get_power(&socket, device, seq).await?;
            // if power == PowerLevel::Standby && power_color.power == PowerLevel::Enabled {
            let off_color = HSBK {
                hue: 0.0,
                saturation: 0.0,
                brightness: 0.0,
                kelvin: 0,
            };
            //     send_set_color(&socket, device, seq, off_color).await?;
            // }

            match power_color {
                PowerColor::Off => {
                    send_set_power(&socket, device, seq, PowerLevel::Off).await?;
                    send_set_color(&socket, device, seq, off_color).await?;
                }
                PowerColor::On(color) => {
                    send_set_power(&socket, device, seq, PowerLevel::On).await?;
                    send_set_colors(color, socket, device, seq, config).await?;
                }
            }
            Ok(())
        } else {
            Err(LifxError::DeviceOffline)
        }
    }
}

async fn send_set_colors(
    color: &Colors,
    socket: UdpSocket,
    device: &mut Device,
    seq: &mut u8,
    config: &DeviceConfig,
) -> Result<(), LifxError> {
    match (config.multiple_zones, color) {
        (true, Colors::Sequence(colors)) => {
            send_set_extended_color_zones(&socket, device, seq, colors).await?;
        }
        (false, Colors::Sequence(colors)) => {
            let first_color = colors.first().unwrap_or_else(|| {
                error!("No colors in sequence");
                &HSBK {
                    hue: 0.0,
                    saturation: 0.0,
                    brightness: 0.0,
                    kelvin: 0,
                }
            });
            send_set_color(&socket, device, seq, *first_color).await?;
        }
        (_, Colors::Single(color)) => {
            send_set_color(&socket, device, seq, *color).await?;
        }
    }
    Ok(())
}

async fn send_set_power(
    socket: &UdpSocket,
    device: &Device,
    sequence: &mut u8,
    power: PowerLevel,
) -> Result<(), LifxError> {
    let level = match power {
        PowerLevel::On => lifx_core::PowerLevel::Enabled,
        PowerLevel::Off => lifx_core::PowerLevel::Standby,
    };
    let msg = Message::SetPower { level };
    send_and_wait_ack(socket, device, sequence, msg).await?;
    Ok(())
}

async fn send_set_color(
    socket: &UdpSocket,
    device: &Device,
    sequence: &mut u8,
    color: HSBK,
) -> Result<(), LifxError> {
    let msg = Message::LightSetColor {
        reserved: 0,
        color: hsbk_to_lifx(color),
        duration: 0,
    };
    send_and_wait_ack(socket, device, sequence, msg).await?;
    Ok(())
}

async fn send_set_extended_color_zones(
    socket: &UdpSocket,
    device: &Device,
    sequence: &mut u8,
    colors: &[HSBK],
) -> Result<(), LifxError> {
    let len = if colors.len() > 82 { 82 } else { colors.len() };

    let c = lifx_core::HSBK {
        hue: 0,
        saturation: 0,
        brightness: 0,
        kelvin: 0,
    };
    let mut x_colors: Box<[lifx_core::HSBK; 82]> = Box::new([c; 82]);

    for (src, dst) in colors.iter().take(len).zip(x_colors.iter_mut()) {
        *dst = hsbk_to_lifx(*src);
    }

    #[allow(clippy::cast_possible_truncation)]
    let msg = Message::SetExtendedColorZones {
        colors: x_colors,
        duration: 0,
        apply: lifx_core::ApplicationRequest::Apply,
        zone_index: 0,
        colors_count: len as u8,
    };
    send_and_wait_ack(socket, device, sequence, msg).await?;
    Ok(())
}

// async fn get_power(
//     socket: &UdpSocket,
//     device: &Device,
//     sequence: &mut u8,
// ) -> Result<PowerLevel, LifxError> {
//     let msg = Message::GetPower;
//     let msg = send_and_wait_response(socket, device, sequence, msg).await?;
//     match msg {
//         Message::StatePower { level: 0 } => Ok(lifx_core::PowerLevel::Standby),
//         Message::StatePower { level: _ } => Ok(lifx_core::PowerLevel::Enabled),
//         _msg => Err(LifxError::BadResponse),
//     }
// }

async fn send_broadcast(
    socket: &UdpSocket,
    msg: Message,
    broadcast: &str,
) -> Result<(), LifxError> {
    let source: u32 = 0x1234_5678;
    let opts = BuildOptions {
        source,
        target: None,
        ack_required: false,
        res_required: false,
        sequence: 0,
    };
    let raw = RawMessage::build(&opts, msg)?;
    let raw = raw.pack()?;
    socket.send_to(&raw, &broadcast).await?;
    Ok(())
}

#[allow(dead_code)]
async fn send_only(
    socket: &UdpSocket,
    device: &Device,
    sequence: &mut u8,
    msg: Message,
) -> Result<(), LifxError> {
    let source: u32 = 0x1234_5678;
    let opts = BuildOptions {
        source,
        target: Some(device.target),
        ack_required: false,
        res_required: false,
        sequence: *sequence,
    };
    let raw = RawMessage::build(&opts, msg)?;
    let raw = raw.pack()?;
    socket.send_to(&raw, &device.addr).await?;
    *sequence = sequence.wrapping_add(1);
    Ok(())
}

#[allow(dead_code)]
async fn send_and_wait_response(
    socket: &UdpSocket,
    device: &Device,
    sequence: &mut u8,
    msg: Message,
) -> Result<Message, LifxError> {
    let mut retries = device.num_retries;

    loop {
        let source: u32 = 0x1234_5678;
        let this_sequence = *sequence;
        *sequence = sequence.wrapping_add(1);

        let opts = BuildOptions {
            source,
            target: Some(device.target),
            ack_required: false,
            res_required: true,
            sequence: this_sequence,
        };
        let raw = RawMessage::build(&opts, msg.clone())?;
        let raw = raw.pack()?;
        socket.send_to(&raw, &device.addr).await?;
        if let Ok(msg) = wait_for_response(socket, this_sequence, device.api_timeout).await {
            break Ok(msg);
        }

        retries = retries.saturating_sub(1);
        if retries == 0 {
            break Err(LifxError::Timeout);
        }
    }
}

async fn send_and_wait_ack(
    socket: &UdpSocket,
    device: &Device,
    sequence: &mut u8,
    msg: Message,
) -> Result<(), LifxError> {
    let mut retries = device.num_retries;

    loop {
        let source: u32 = 0x1234_5678;
        let this_sequence = *sequence;
        *sequence = sequence.wrapping_add(1);

        let opts = BuildOptions {
            source,
            target: Some(device.target),
            ack_required: true,
            res_required: false,
            sequence: this_sequence,
        };
        let raw = RawMessage::build(&opts, msg.clone())?;
        let raw = raw.pack()?;
        socket.send_to(&raw, &device.addr).await?;
        if (wait_for_response(socket, this_sequence, device.api_timeout).await).is_ok() {
            return Ok(());
        }

        retries = retries.saturating_sub(1);
        if retries == 0 {
            break Err(LifxError::Timeout);
        }
    }
}

async fn wait_for_response(
    socket: &UdpSocket,
    sequence: u8,
    timeout: Duration,
) -> Result<Message, LifxError> {
    let mut buf = [0; 1024];
    loop {
        let timeout = Instant::now() + timeout;
        select! {
            _ = sleep_until(timeout) => {
                return Err(LifxError::Timeout);
            }
            Ok((len, _)) = socket.recv_from(&mut buf) => {
                let raw = RawMessage::unpack(&buf[..len])?;
                if raw.frame_addr.sequence == sequence {
                    return Ok(Message::from_raw(&raw)?);
                }
            }
        }
    }
}

/// Configuration for a device.
#[derive(Default)]
pub struct DeviceConfig {
    /// Does this device have multiple zones?
    pub multiple_zones: bool,
}

/// Run the device.
///
/// # Panics
///
/// This function will panic if something goes wrong.
pub fn device_entity(
    rx_pc: entities::Receiver<PowerColor>,
    tx_state: entities::Sender<State>,
    id: u64,
    discover: entities::Receiver<Device>,
    config: DeviceConfig,
) {
    let discover = discover.filter_into_stateless(move |d| d.target == id);

    spawn(async move {
        let mut discover_s = discover.subscribe().await;
        let mut rx_s = rx_pc.subscribe().await;
        let mut state = DeviceState::Offline;
        let mut power_color = PowerColor::Off;
        tx_state.try_send(State::Offline);

        loop {
            select! {
                Ok(d) = discover_s.recv() => {
                    state.set_online(d);
                    match state.set_power_color(&power_color, &config).await {
                        Ok(_) => {
                            state.renew_online();
                            debug!("{id} discovered and initializing: {power_color:?}");
                            tx_state.try_send(State::Online(power_color.clone()));
                        }
                        Err(err) => {
                            state.set_offline();
                            info!("{id} failed initialize: {err:?}");
                            tx_state.try_send(State::Offline);
                        }
                    }
                }
                Ok(pc) = rx_s.recv() => {
                    power_color = pc;
                    match state.set_power_color(&power_color, &config).await {
                        Ok(_) => {
                            state.renew_online();
                            debug!("{id} set power color: {power_color:?}");
                            tx_state.try_send(State::Online(power_color.clone()));
                        }
                        Err(err) => {
                            state.set_offline();
                            info!("{id} failed to set power color: {err:?}");
                            tx_state.try_send(State::Offline);
                        }
                    }
                }
                Some(_) = maybe_sleep_until(&state) => {
                    match state.set_power_color(&power_color, &config).await {
                        Ok(_) => {
                            state.renew_online();
                            debug!("{id} timeout check: {power_color:?}");
                            tx_state.try_send(State::Online(power_color.clone()));
                        }
                        Err(err) => {
                            state.set_offline();
                            info!("{id} failed to timeout check: {err:?}");
                            tx_state.try_send(State::Offline);
                        }
                    }
                }
            }
        }
    });
}

async fn maybe_sleep_until(state: &DeviceState) -> Option<()> {
    if let DeviceState::Online(_, timeout, _) = state {
        sleep_until(*timeout).await;
        Some(())
    } else {
        None
    }
}
