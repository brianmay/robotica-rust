use paho_mqtt::Message;
use tokio::sync::mpsc::{Receiver, Sender};

use self::mqtt::MqttMessage;

pub mod life360;
pub mod mqtt;

pub trait ChainMqtt {
    fn publish(self, mqtt_out: Sender<MqttMessage>);
}

impl ChainMqtt for Receiver<Message> {
    fn publish(self, mqtt_out: tokio::sync::mpsc::Sender<MqttMessage>) {
        mqtt::publish(self, mqtt_out)
    }
}
