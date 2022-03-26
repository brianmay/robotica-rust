mod flows;

use anyhow::Result;
use flows::google;
use flows::life360;
use flows::tesla;
use tokio::sync::mpsc;

use robotica_node_rust::sources::mqtt::{Mqtt, MqttMessage, Subscriptions};

// #[derive(Clone, Debug, PartialEq, Eq)]
// enum Power {
//     On,
//     Off,
//     HardOff,
//     Error,
// }

// fn power_to_enum(value: String) -> Power {
//     match value.as_str() {
//         "OFF" => Power::Off,
//         "ON" => Power::On,
//         "HARD_OFF" => Power::HardOff,
//         _ => Power::Error,
//     }
// }

// fn changed_to_string(value: (Power, Power)) -> Option<String> {
//     match value {
//         (Power::Error, _) => None,
//         (_, Power::Error) => None,
//         (_, Power::Off) => Some("Fan has been turned off".to_string()),
//         (_, Power::On) => Some("Fan has been turned on".to_string()),
//         (_, Power::HardOff) => Some("Fan has been turned off at power point".to_string()),
//     }
// }

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let mut mqtt = Mqtt::new().await;
    let tx = mqtt.take_tx()?;

    let subscriptions: Subscriptions = setup_pipes(&tx);
    mqtt.connect(subscriptions);

    drop(mqtt);
    Ok(())
}

fn setup_pipes(mqtt: &mpsc::Sender<MqttMessage>) -> Subscriptions {
    let mut subscriptions: Subscriptions = Subscriptions::new();

    tesla::start(&mut subscriptions, mqtt, 1);
    life360::start(&mut subscriptions, mqtt);
    google::start(&mut subscriptions, mqtt);

    // subscriptions
    //     .subscribe("state/Brian/Fan/power")
    //     .map(power_to_enum)
    //     .has_changed()
    //     .filter_map(changed_to_string)
    //     .message(&mut subscriptions, mqtt);

    subscriptions
}
