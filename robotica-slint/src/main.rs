//! Code to deploy on Robotica Pis for audio and UI
#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod audio;
mod command;
mod config;
mod duration;
mod partial_command;
mod ui;

use std::sync::Arc;

use robotica_backend::{
    pipes::stateless::{self, Started},
    services::{
        mqtt::{self, mqtt_channel, run_client, MqttTx, Subscriptions},
        persistent_state::{self, PersistentStateDatabase},
    },
};
use robotica_common::version;
use tokio::sync::mpsc;

use tracing::info;
use ui::ScreenCommand;

struct LoadedConfig {
    ui: Arc<ui::LoadedConfig>,
    audio: Arc<audio::LoadedConfig>,
    persistent_state: persistent_state::Config,
    mqtt: mqtt::Config,
}

impl TryFrom<config::Config> for LoadedConfig {
    type Error = anyhow::Error;

    fn try_from(config: config::Config) -> Result<Self, Self::Error> {
        let ui = Arc::new(ui::LoadedConfig::try_from(config.ui)?);
        let audio = Arc::new(audio::LoadedConfig::try_from(config.audio)?);
        let persistent_state = config.persistent_state;
        let mqtt = config.mqtt;
        Ok(Self {
            ui,
            audio,
            persistent_state,
            mqtt,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();
    color_backtrace::install();
    let started = stateless::Started::new();

    info!(
        "Starting robotica-slint, version = {:?}, build time = {:?}",
        version::VCS_REF,
        version::BUILD_DATE
    );

    info!(
        "Starting brian-backend, version = {:?}, build time = {:?}",
        version::VCS_REF,
        version::BUILD_DATE
    );

    let env = config::Environment::load().unwrap_or_else(|e| {
        panic!("Error loading environment: {e}");
    });

    let config = env.config().unwrap_or_else(|e| {
        panic!("Error loading config: {e}");
    });

    let config: LoadedConfig = config.try_into()?;
    start_services(config, &started)?;

    Ok(())
}

/// Running state for program.
pub struct RunningState {
    mqtt: MqttTx,
    tx_screen_command: mpsc::Sender<ScreenCommand>,
    // config: Arc<Config>,
    // persistent_state_database: PersistentStateDatabase,
}

fn start_services(config: LoadedConfig, started: &Started) -> Result<(), anyhow::Error> {
    let (mqtt, mqtt_rx) = mqtt_channel();
    let mut subscriptions: Subscriptions = Subscriptions::new();
    let persistent_state_database = PersistentStateDatabase::new(&config.persistent_state)
        .unwrap_or_else(|e| {
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

    let mqtt_config = config.mqtt;
    run_client(subscriptions, mqtt_rx, mqtt_config)?;

    let running_state = RunningState {
        mqtt,
        tx_screen_command,
    };

    started.notify();

    ui::run_gui(running_state, config.ui, rx_screen_command);
    Ok(())
}
