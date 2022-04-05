use paho_mqtt::Message;

use crate::RxPipe;

use self::mqtt::MqttOut;

pub mod life360;
pub mod mqtt;
pub mod timer;

impl RxPipe<Message> {
    pub fn publish(self, mqtt_out: &MqttOut) {
        mqtt::publish(self.subscribe(), mqtt_out)
    }
}
