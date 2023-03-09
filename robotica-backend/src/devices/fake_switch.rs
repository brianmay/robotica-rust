//! Fake a digital on/off switch
use robotica_common::{
    mqtt::{Json, MqttMessage, QoS},
    robotica::commands::{Command, DeviceAction, DevicePower},
};
use tokio::select;

use crate::{
    services::mqtt::{MqttTx, Subscriptions},
    spawn,
};

/// Run a fake switch
pub fn run(subscription: &mut Subscriptions, mqtt: MqttTx, topic_substr: impl Into<String>) {
    let topic_substr: String = topic_substr.into();
    let topic = format!("command/{topic_substr}");
    let rx = subscription.subscribe_into_stateless::<Json<Command>>(&topic);

    spawn(async move {
        let mut rx = rx.subscribe().await;

        loop {
            select! {
                Ok(Json(command)) = rx.recv() => {
                    match command {
                        Command::Device(command) => {
                            let topic = format!("state/{topic_substr}/power");
                            let status = match command.action {
                                DeviceAction::TurnOff => DevicePower::Off,
                                DeviceAction::TurnOn => DevicePower::On,
                            };
                            let string: String = status.into();
                            let msg = MqttMessage::new(topic, string, true, QoS::AtLeastOnce);
                            mqtt.try_send(msg);
                        },
                        command => {
                            tracing::error!("Invalid command, expected switch, got {:?}", command);
                        }
                    }
                },
                else => break,
            };
        }
    });
}
