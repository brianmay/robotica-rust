mod delays;
mod hdmi;
mod robotica;
mod tesla;

use anyhow::Result;
use robotica_backend::entities::Sender;
use robotica_backend::scheduling::executor::executor;

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

pub struct State {
    subscriptions: Subscriptions,
    #[allow(dead_code)]
    mqtt: Mqtt,
    message_sink: Sender<String>,
}

async fn setup_pipes(mqtt: Mqtt) -> Subscriptions {
    let mut subscriptions: Subscriptions = Subscriptions::new();

    let message_sink = robotica::create_message_sink(&mut subscriptions, mqtt.clone());

    let mut state = State {
        subscriptions,
        mqtt,
        message_sink,
    };

    http::run(state.mqtt.clone())
        .await
        .expect("HTTP server failed");
    hdmi::run(&mut state, "Dining", "TV", "hdmi.pri:8000");
    tesla::monitor_tesla_doors(&mut state, 1);

    executor(&mut state.subscriptions, state.mqtt).unwrap_or_else(|err| {
        panic!("Failed to start executor: {}", err);
    });

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
