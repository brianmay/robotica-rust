mod flows;
mod http;

use anyhow::Result;
use flows::common::message_sink;
use flows::google;
use flows::life360;
use flows::tesla;
use flows::zigbee;
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
    let message_sink = message_sink(&mut subscriptions, mqtt);

    tesla::start(&mut subscriptions, &message_sink);
    life360::start(mqtt, &message_sink);
    zigbee::start(&mut subscriptions, &message_sink, mqtt);
    google::start(&mut subscriptions, mqtt);

    subscriptions
}
