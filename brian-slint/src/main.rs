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

#[tokio::main]
async fn main() -> Result<(), Box<anyhow::Error>> {
    tracing_subscriber::fmt::init();
    color_backtrace::install();

    let args: Vec<String> = std::env::args().collect();
    let location = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("No location provided"))?;

    start_services(location)?;
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
    // persistent_state_database: PersistentStateDatabase,
    location: String,
}

fn start_services(location: impl Into<String>) -> Result<(), anyhow::Error> {
    let (mqtt, mqtt_rx) = mqtt_channel();
    let subscriptions: Subscriptions = Subscriptions::new();
    let persistent_state_database = PersistentStateDatabase::new().unwrap_or_else(|e| {
        panic!("Error getting persistent state loader: {e}");
    });

    let mut state = SetupState {
        subscriptions,
        mqtt,
        persistent_state_database,
        location: location.into(),
    };

    setup_pipes(&mut state);

    run_client(state.subscriptions, mqtt_rx)?;

    let running_state = RunningState {
        mqtt: state.mqtt,
        // persistent_state_database: state.persistent_state_database,
        location: state.location,
    };

    ui::run_gui(&Arc::new(running_state));
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

#[allow(dead_code)]
struct LabeledButtonConfig {
    bc: ButtonConfig,
    title: String,
    icon: Icon,
}
