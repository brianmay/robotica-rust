//! Code to deploy on Robotica Pis for audio and UI
#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod audio;
mod command;
mod duration;
mod ui;

use std::sync::Arc;

use robotica_backend::services::{
    mqtt::{mqtt_channel, run_client, MqttTx, Subscriptions},
    persistent_state::PersistentStateDatabase,
};
use robotica_common::controllers::{lights2, switch};
use serde::Deserialize;
use tokio::sync::mpsc;
use ui::{ScreenCommand, WidgetConfig};

#[derive(Deserialize)]
struct Config {
    number_per_row: u8,
    buttons: Vec<WidgetConfig>,
    audio: Arc<audio::Config>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();
    color_backtrace::install();

    let args: Vec<String> = std::env::args().collect();
    let config_file = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("No location provided"))?;

    let string = std::fs::read_to_string(config_file)?;
    let config: Arc<Config> = Arc::new(serde_yaml::from_str(&string)?);
    start_services(&config)?;

    Ok(())
}

struct SetupState {
    subscriptions: Subscriptions,
    mqtt: MqttTx,
    persistent_state_database: PersistentStateDatabase,
    config: Arc<Config>,
    tx_screen_command: mpsc::Sender<ScreenCommand>,
}

/// Running state for program.
pub struct RunningState {
    mqtt: MqttTx,
    tx_screen_command: mpsc::Sender<ScreenCommand>,
    // config: Arc<Config>,
    // persistent_state_database: PersistentStateDatabase,
}

fn start_services(config: &Arc<Config>) -> Result<(), anyhow::Error> {
    let (mqtt, mqtt_rx) = mqtt_channel();
    let subscriptions: Subscriptions = Subscriptions::new();
    let persistent_state_database = PersistentStateDatabase::new().unwrap_or_else(|e| {
        panic!("Error getting persistent state loader: {e}");
    });

    let (tx_screen_command, rx_screen_command) = mpsc::channel(1);

    let mut state = SetupState {
        subscriptions,
        mqtt,
        persistent_state_database,
        config: config.clone(),
        tx_screen_command,
    };

    setup_pipes(&mut state);

    run_client(state.subscriptions, mqtt_rx)?;

    let running_state = RunningState {
        mqtt: state.mqtt,
        tx_screen_command: state.tx_screen_command,
    };

    ui::run_gui(
        running_state,
        config.number_per_row,
        &config.buttons,
        rx_screen_command,
    );
    Ok(())
}

fn setup_pipes(state: &mut SetupState) {
    audio::run(
        state.tx_screen_command.clone(),
        &mut state.subscriptions,
        state.mqtt.clone(),
        &state.persistent_state_database,
        state.config.audio.clone(),
    );
}

#[allow(dead_code)]
enum ButtonConfig {
    Light2Config(lights2::Config),
    DeviceConfig(switch::Config),
}

#[allow(dead_code)]
enum Icon {
    Light,
    Fan,
}
