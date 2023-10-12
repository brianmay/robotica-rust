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

use envconfig::Envconfig;
use robotica_backend::services::{
    mqtt::{self, mqtt_channel, run_client, MqttTx, Subscriptions},
    persistent_state::{self, PersistentStateDatabase},
};
use serde::Deserialize;
use tokio::sync::mpsc;

use ui::ScreenCommand;

/// Environment variables for the application.
#[derive(Envconfig)]
pub struct Environment {
    /// The MQTT username.
    #[envconfig(from = "MQTT_USERNAME")]
    pub mqtt_username: String,

    /// The MQTT password.
    #[envconfig(from = "MQTT_PASSWORD")]
    pub mqtt_password: String,

    /// The MQTT host.
    #[envconfig(from = "MQTT_HOST")]
    pub mqtt_host: String,

    /// The MQTT port.
    #[envconfig(from = "MQTT_PORT")]
    pub mqtt_port: u16,
}

impl Environment {
    /// Load the environment from the environment variables.
    ///
    /// # Errors
    ///
    /// Will return an error if the environment variables are not set.
    pub fn load() -> Result<Self, envconfig::Error> {
        Self::init_from_env()
    }
}

#[derive(Deserialize)]
struct Config {
    ui: ui::Config,
    audio: audio::Config,
    persistent_state: persistent_state::Config,
}

struct LoadedConfig {
    ui: Arc<ui::LoadedConfig>,
    audio: Arc<audio::LoadedConfig>,
    persistent_state: persistent_state::Config,
}

impl TryFrom<Config> for LoadedConfig {
    type Error = anyhow::Error;

    fn try_from(config: Config) -> Result<Self, Self::Error> {
        let ui = Arc::new(ui::LoadedConfig::try_from(config.ui)?);
        let audio = Arc::new(audio::LoadedConfig::try_from(config.audio)?);
        let persistent_state = config.persistent_state;
        Ok(Self {
            ui,
            audio,
            persistent_state,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();
    color_backtrace::install();

    let env = Environment::load()?;

    let args: Vec<String> = std::env::args().collect();
    let config_file = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("No location provided"))?;

    let string = std::fs::read_to_string(config_file)?;
    let config: Config = serde_yaml::from_str(&string)?;
    let config: LoadedConfig = config.try_into()?;
    start_services(&env, &config)?;

    Ok(())
}

/// Running state for program.
pub struct RunningState {
    mqtt: MqttTx,
    tx_screen_command: mpsc::Sender<ScreenCommand>,
    // config: Arc<Config>,
    // persistent_state_database: PersistentStateDatabase,
}

fn start_services(env: &Environment, config: &LoadedConfig) -> Result<(), anyhow::Error> {
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

    let mqtt_config = mqtt::Config {
        mqtt_host: env.mqtt_host.clone(),
        mqtt_port: env.mqtt_port,
        mqtt_username: env.mqtt_username.clone(),
        mqtt_password: env.mqtt_password.clone(),
    };
    run_client(subscriptions, mqtt_rx, mqtt_config)?;

    let running_state = RunningState {
        mqtt,
        tx_screen_command,
    };

    ui::run_gui(running_state, config.ui.clone(), rx_screen_command);
    Ok(())
}
