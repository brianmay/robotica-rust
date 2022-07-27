mod flows;
mod http;

use anyhow::Result;
use flows::common::message_sink;
use flows::google;
use robotica_node_rust::sources::mqtt::MqttOut;

use robotica_node_rust::sources::mqtt::{MqttClient, Subscriptions};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    http::start().await;

    let mut mqtt = MqttClient::new().await;
    let tx = mqtt.get_mqtt_out();

    let subscriptions: Subscriptions = setup_pipes(&tx);
    mqtt.connect(subscriptions);
    mqtt.wait().await;

    Ok(())
}

fn setup_pipes(mqtt: &MqttOut) -> Subscriptions {
    let mut subscriptions: Subscriptions = Subscriptions::new();
    let _message_sink = message_sink(&mut subscriptions, mqtt);

    google::start(&mut subscriptions, mqtt);

    subscriptions
}
