//! Code to deploy on Robotica Pis for audio and UI
#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod audio;
mod command;
mod ui;

use std::sync::Arc;

use robotica_backend::services::{
    mqtt::{mqtt_channel, run_client, MqttTx, Subscriptions},
    persistent_state::PersistentStateDatabase,
};
use robotica_common::controllers::{lights2, switch};
use serde::Deserialize;
use ui::LabeledButtonConfig;

#[derive(Deserialize)]
struct Config {
    location: String,
    buttons: Vec<Arc<LabeledButtonConfig>>,
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
    start_services(config)?;

    Ok(())
}

struct SetupState {
    subscriptions: Subscriptions,
    mqtt: MqttTx,
    persistent_state_database: PersistentStateDatabase,
    location: String,
}

/// Running state for program.
pub struct RunningState {
    mqtt: MqttTx,
    // config: Config,
    // persistent_state_database: PersistentStateDatabase,
    // location: String,
}

fn start_services(config: Config) -> Result<(), anyhow::Error> {
    let (mqtt, mqtt_rx) = mqtt_channel();
    let subscriptions: Subscriptions = Subscriptions::new();
    let persistent_state_database = PersistentStateDatabase::new().unwrap_or_else(|e| {
        panic!("Error getting persistent state loader: {e}");
    });

    let mut state = SetupState {
        subscriptions,
        mqtt,
        persistent_state_database,
        location: config.location.clone(),
    };

    setup_pipes(&mut state);

    run_client(state.subscriptions, mqtt_rx)?;

    let running_state = RunningState {
        mqtt: state.mqtt,
        // persistent_state_database: state.persistent_state_database,
        // location: state.location,
        // config,
    };

    ui::run_gui(running_state, config.buttons);
    Ok(())
}

fn setup_pipes(state: &mut SetupState) {
    let topic_substr = format!("{}/Robotica", state.location);

    audio::run(
        &mut state.subscriptions,
        state.mqtt.clone(),
        &state.persistent_state_database,
        topic_substr,
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
