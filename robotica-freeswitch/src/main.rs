//! Code to deploy on Robotica Pis for audio and UI
#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod config;
mod freeswitch;
mod phone_db;

use config::Config;
use robotica_common::version;
use robotica_tokio::{
    pipes::stateless,
    services::mqtt::{mqtt_channel, run_client, MqttTx, Subscriptions},
};

use tracing::{debug, info};

use crate::config::Environment;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();
    color_backtrace::install();
    let started = stateless::Started::new();

    info!(
        "Starting Freeswitch, version = {:?}, build time = {:?}",
        version::VCS_REF,
        version::BUILD_DATE
    );

    let env = Environment::load()?;
    let config = env.config()?;
    start_services(config).await?;

    started.notify();
    loop {
        debug!("I haven't crashed yet!");
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
    }
}

/// Running state for program.
pub struct RunningState {
    mqtt: MqttTx,
    subscriptions: Subscriptions,
}

async fn start_services(config: Config) -> Result<(), anyhow::Error> {
    let (mqtt, mqtt_rx) = mqtt_channel();
    let subscriptions: Subscriptions = Subscriptions::new();

    let running_state = RunningState {
        mqtt,
        subscriptions,
    };

    freeswitch::run(&running_state, config.freeswitch, config.phone_db).await?;
    run_client(running_state.subscriptions, mqtt_rx, config.mqtt)?;

    Ok(())
}
