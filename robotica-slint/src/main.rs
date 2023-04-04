//! Code to deploy on Robotica Pis for audio and UI
#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod audio;
mod command;
mod duration;
mod partial_command;
mod ui;

use std::sync::Arc;

use robotica_backend::services::{
    mqtt::{mqtt_channel, run_client, MqttTx, Subscriptions},
    persistent_state::PersistentStateDatabase,
};
use robotica_common::controllers::{lights2, switch};
use serde::Deserialize;
use tokio::sync::mpsc;

use ui::ScreenCommand;

#[derive(Deserialize)]
struct Config {
    ui: ui::Config,
    audio: audio::Config,
}

struct LoadedConfig {
    ui: Arc<ui::LoadedConfig>,
    audio: Arc<audio::LoadedConfig>,
}

impl TryFrom<Config> for LoadedConfig {
    type Error = anyhow::Error;

    fn try_from(config: Config) -> Result<Self, Self::Error> {
        let ui = Arc::new(ui::LoadedConfig::try_from(config.ui)?);
        let audio = Arc::new(audio::LoadedConfig::try_from(config.audio)?);
        Ok(Self { ui, audio })
    }
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
    let config: Config = serde_yaml::from_str(&string)?;
    let config: LoadedConfig = config.try_into()?;
    start_services(&config)?;

    Ok(())
}

/// Running state for program.
pub struct RunningState {
    mqtt: MqttTx,
    tx_screen_command: mpsc::Sender<ScreenCommand>,
    // config: Arc<Config>,
    // persistent_state_database: PersistentStateDatabase,
}

fn start_services(config: &LoadedConfig) -> Result<(), anyhow::Error> {
    let (mqtt, mqtt_rx) = mqtt_channel();
    let mut subscriptions: Subscriptions = Subscriptions::new();
    let persistent_state_database = PersistentStateDatabase::new().unwrap_or_else(|e| {
        panic!("Error getting persistent state loader: {e}");
    });

    let (tx_screen_command, rx_screen_command) = mpsc::channel(1);

    audio::run(
        tx_screen_command.clone(),
        &mut subscriptions,
        mqtt.clone(),
        &persistent_state_database,
        config.audio.clone(),
    );

    run_client(subscriptions, mqtt_rx)?;

    let running_state = RunningState {
        mqtt,
        tx_screen_command,
    };

    ui::run_gui(running_state, config.ui.clone(), rx_screen_command);
    Ok(())
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
