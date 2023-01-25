//! Main entry point for the application.

#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod amber;
mod delays;
mod environment_monitor;
mod hdmi;
mod robotica;
mod tesla;

use anyhow::Result;
use robotica_backend::devices::fake_switch;
use robotica_backend::entities::Sender;
use robotica_backend::scheduling::executor::executor;
use robotica_backend::services::persistent_state::PersistentStateDatabase;

use self::tesla::monitor_charging;
use robotica_backend::services::http;
use robotica_backend::services::mqtt::Mqtt;
use robotica_backend::services::mqtt::{MqttClient, Subscriptions};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    color_backtrace::install();

    let (mqtt_client, mqtt) = MqttClient::new();

    let subscriptions: Subscriptions = setup_pipes(mqtt).await;
    mqtt_client.do_loop(subscriptions).await?;

    Ok(())
}

/// Global state for the application.
pub struct State {
    subscriptions: Subscriptions,
    #[allow(dead_code)]
    mqtt: Mqtt,
    message_sink: Sender<String>,
    persistent_state_database: PersistentStateDatabase,
}

async fn setup_pipes(mqtt: Mqtt) -> Subscriptions {
    let mut subscriptions: Subscriptions = Subscriptions::new();

    let message_sink = robotica::create_message_sink(&mut subscriptions, mqtt.clone());
    let persistent_state_database = PersistentStateDatabase::new().unwrap_or_else(|e| {
        panic!("Error getting persistent state loader: {e}");
    });

    let mut state = State {
        subscriptions,
        mqtt,
        message_sink,
        persistent_state_database,
    };

    let price_summary_rx = amber::run(&state).unwrap_or_else(|e| {
        panic!("Error running amber: {e}");
    });

    {
        price_summary_rx
            .clone()
            .map_into_stateful(|(_, current)| current.is_cheap_2hr)
            .for_each(move |(old, current)| {
                if old.is_some() {
                    let message = if current {
                        "2 hour cheap price has started"
                    } else {
                        "2 hour cheap price has ended"
                    };
                    log::info!("{}", message);
                }
            });
    }

    monitor_charging(&mut state, 1, price_summary_rx).unwrap_or_else(|e| {
        panic!("Error running tesla charging monitor: {e}");
    });

    http::run(state.mqtt.clone())
        .await
        .unwrap_or_else(|e| panic!("Error running http server: {e}"));

    hdmi::run(&mut state, "Dining", "TV", "hdmi.pri:8000");
    tesla::monitor_tesla_doors(&mut state, 1);

    environment_monitor::run(&mut state).unwrap_or_else(|err| {
        panic!("Environment monitor failed: {err}");
    });

    executor(&mut state.subscriptions, state.mqtt.clone()).unwrap_or_else(|err| {
        panic!("Failed to start executor: {err}");
    });

    fake_switch::run(
        &mut state.subscriptions,
        state.mqtt.clone(),
        "Dining/Messages",
    );

    fake_switch::run(
        &mut state.subscriptions,
        state.mqtt.clone(),
        "Dining/Request_Bathroom",
    );

    fake_switch::run(&mut state.subscriptions, state.mqtt.clone(), "Brian/Night");

    fake_switch(&mut state, "Dining/Messages");
    fake_switch(&mut state, "Dining/Request_Bathroom");
    fake_switch(&mut state, "Brian/Night");
    fake_switch(&mut state, "Brian/Messages");
    fake_switch(&mut state, "Brian/Request_Bathroom");

    // let message_sink_temp = state.message_sink.clone();
    // let rx = state
    //     .subscriptions
    //     .subscribe_into::<Power>("state/Brian/Light/power");
    // spawn(async move {
    //     let mut s = rx.subscribe().await;
    //     loop {
    //         let msg = s.recv().await;
    //         if let Ok((Some(prev), current)) = msg {
    //             let announce = format!("Light power changed from {} to {}", prev, current);

    //             if let Err(err) = message_sink_temp.send(announce).await {
    //                 error!("Error sending message: {}", err);
    //             }
    //         }
    //         if let Some(msg) = rx.get().await {
    //             debug!("get: {:?}", msg);
    //         }
    //     }
    // });

    state.subscriptions
}

fn fake_switch(state: &mut State, topic_substr: &str) {
    fake_switch::run(&mut state.subscriptions, state.mqtt.clone(), topic_substr);
}
