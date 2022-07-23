use anyhow::Result;
use chrono::DateTime;
use chrono::Timelike;
use chrono::Utc;
use chrono_tz::Tz;

use log::*;
use paho_mqtt::Message;
use robotica_node_rust::filters::generic::if_else;
use robotica_node_rust::recv;
use robotica_node_rust::send_or_log;
use robotica_node_rust::sources::mqtt::MqttOut;
use robotica_node_rust::sources::mqtt::Subscriptions;
use robotica_node_rust::sources::timer;
use robotica_node_rust::spawn;
use robotica_node_rust::Pipe;
use robotica_node_rust::RxPipe;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::select;

use super::espresence;
use super::robotica::string_to_power;
use super::robotica::Action;
use super::robotica::Id;
use super::robotica::Power;
use super::robotica::RoboticaAutoColor;
use super::robotica::RoboticaAutoColorOut;
use super::robotica::RoboticaColorOut;
use super::robotica::RoboticaDeviceCommand;
use super::robotica::RoboticaLightCommand;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct GoogleColor {
    hue: Option<u16>,
    saturation: Option<u16>,
    brightness: Option<u16>,
    temperature: Option<u16>,
    on: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct GoogleCommand {
    on: bool,
    online: bool,
}

pub fn start(subscriptions: &mut Subscriptions, mqtt_out: &MqttOut) {
    light(&Id::new("Brian", "Light"), subscriptions, mqtt_out);
    light(&Id::new("Dining", "Light"), subscriptions, mqtt_out);
    light(&Id::new("Passage", "Light"), subscriptions, mqtt_out);
    light(&Id::new("Twins", "Light"), subscriptions, mqtt_out);
    light(&Id::new("Akira", "Light"), subscriptions, mqtt_out);

    device(&Id::new("Brian", "Fan"), subscriptions, mqtt_out);
    device(&Id::new("Dining", "TvSwitch"), subscriptions, mqtt_out);
}

fn light_google_to_robotica(payload: String, id: &Id) -> Option<Message> {
    let mut color = RoboticaColorOut {
        hue: 0,
        saturation: 0,
        brightness: 0,
        kelvin: 3500,
    };

    let d = &mut serde_json::Deserializer::from_str(&payload);
    let gc: Result<GoogleColor, _> = serde_path_to_error::deserialize(d);

    if let Err(err) = gc {
        error!("light_google_to_robotica: {err}");
        return None;
    }

    let gc = gc.unwrap();

    if let Some(hue) = gc.hue {
        color.hue = hue;
    }

    if let Some(saturation) = gc.saturation {
        color.saturation = saturation;
    }

    if let Some(brightness) = gc.brightness {
        color.brightness = brightness;
    }

    if let Some(temperature) = gc.temperature {
        color.kelvin = temperature;
    }

    let action = if gc.on { None } else { Some(Action::TurnOff) };

    let scene = match (id.location.as_str(), id.device.as_str()) {
        ("Brian", "Light") => "auto".to_string(),
        (_, _) => "default".to_string(),
    };

    let command = RoboticaLightCommand {
        action,
        color: Some(color),
        scene: Some(scene),
    };

    let topic = id.get_command_topic(&[]);
    let payload = serde_json::to_string(&command).unwrap();
    Some(Message::new(topic, payload, 0))
}

fn device_google_to_robotica(payload: String, id: &Id) -> Option<Message> {
    let d = &mut serde_json::Deserializer::from_str(&payload);
    let gc: Result<GoogleCommand, _> = serde_path_to_error::deserialize(d);

    if let Err(err) = gc {
        error!("device_google_to_robotica: {err}");
        return None;
    }

    let gc = gc.unwrap();
    let action = if gc.on {
        Some(Action::TurnOn)
    } else {
        Some(Action::TurnOff)
    };

    let command = RoboticaDeviceCommand { action };
    let topic = id.get_command_topic(&[]);
    let payload = serde_json::to_string(&command).unwrap();
    Some(Message::new(topic, payload, 0))
}

fn robotica_to_google(power: Power, id: &Id) -> Message {
    let command = match power {
        Power::On => GoogleCommand {
            on: true,
            online: true,
        },
        Power::Off => GoogleCommand {
            on: false,
            online: true,
        },
        Power::HardOff => GoogleCommand {
            on: false,
            online: false,
        },
        Power::Error => GoogleCommand {
            on: false,
            online: false,
        },
    };

    let topic = id.get_google_in_topic();
    let payload = serde_json::to_string(&command).unwrap();
    Message::new(topic, payload, 0)
}

fn timer_to_color(id: &Id) -> RoboticaAutoColor {
    let now: DateTime<Utc> = Utc::now();
    let tz: Tz = "Australia/Melbourne".parse().unwrap();
    let local_now = now.with_timezone(&tz);
    let hour = local_now.hour();

    let brightness = if id.location == "Brian" {
        match hour {
            h if !(5..22).contains(&h) => 5,
            h if !(6..21).contains(&h) => 15,
            h if !(7..20).contains(&h) => 25,
            h if !(8..19).contains(&h) => 50,
            h if !(9..18).contains(&h) => 100,
            _ => 100,
        }
    } else {
        100
    };

    let kelvin = match hour {
        h if !(5..22).contains(&h) => 1000,
        h if !(6..21).contains(&h) => 1500,
        h if !(7..20).contains(&h) => 2000,
        h if !(8..19).contains(&h) => 2500,
        h if !(9..18).contains(&h) => 3000,
        _ => 3500,
    };

    RoboticaAutoColor {
        power: Some(Power::On),
        color: RoboticaAutoColorOut {
            hue: Some(0),
            saturation: Some(0),
            brightness: Some(brightness),
            kelvin: Some(kelvin),
            alpha: Some(100),
        },
    }
}

fn color_to_message(color: RoboticaAutoColor, id: &Id) -> Message {
    let topic = id.get_command_topic(&["scene", "auto"]);
    let payload = serde_json::to_string(&color).unwrap();
    Message::new_retained(topic, payload, 0)
}

fn light(id: &Id, subscriptions: &mut Subscriptions, mqtt_out: &MqttOut) {
    {
        let id = (*id).clone();
        let topic = id.get_google_out_topic();
        subscriptions
            .subscribe_to_string(&topic)
            .filter_map(move |payload| light_google_to_robotica(payload, &id))
            .publish(mqtt_out);
    }

    {
        let topic = id.get_state_topic("power");
        let power_str = subscriptions.subscribe_to_string(&topic);

        let topic = id.get_state_topic("priorities");
        let priorities = subscriptions.subscribe_to_string(&topic).map(|payload| {
            let list: Vec<u16> = serde_json::from_str(&payload).unwrap();
            list
        });

        let id = (*id).clone();
        light_power(priorities, power_str)
            .map(move |power| robotica_to_google(power, &id))
            .publish(mqtt_out);
    }

    let gate = match id.location.as_str() {
        "Brian" => espresence::brian_in_room("brian", subscriptions, 20.0),
        "Passage" => espresence::brian_in_room("passage", subscriptions, 1.5),
        _ => timer::timer(Duration::from_secs(60), true),
    };

    {
        let off_color = timer::timer(
            Duration::from_secs(60),
            RoboticaAutoColor {
                power: None,
                color: RoboticaAutoColorOut {
                    hue: Some(0),
                    saturation: Some(0),
                    brightness: Some(0),
                    kelvin: Some(3500),
                    alpha: Some(0),
                },
            },
        );

        let id1 = (*id).clone();
        let on_color =
            timer::timer(Duration::from_secs(60), true).map(move |_| timer_to_color(&id1));

        let id2 = (*id).clone();
        if_else(gate, on_color, off_color)
            .diff()
            .changed_or_unknown()
            .map(move |c| color_to_message(c, &id2))
            .publish(mqtt_out);
    }
}

fn device(id: &Id, subscriptions: &mut Subscriptions, mqtt_out: &MqttOut) {
    {
        let id = (*id).clone();
        let topic = id.get_google_out_topic();

        subscriptions
            .subscribe_to_string(&topic)
            .filter_map(move |payload| device_google_to_robotica(payload, &id))
            .publish(mqtt_out);
    }

    {
        let id = (*id).clone();
        let topic = id.get_state_topic("power");

        subscriptions
            .subscribe_to_string(&topic)
            .map(string_to_power)
            .map(move |power| robotica_to_google(power, &id))
            .publish(mqtt_out);
    }
}

fn light_power(priorities: RxPipe<Vec<u16>>, power: RxPipe<String>) -> RxPipe<Power> {
    let output = Pipe::new();
    let tx = output.get_tx();
    let mut priorities = priorities.subscribe();
    let mut power = power.subscribe();

    spawn(async move {
        let mut the_priorities: Option<Vec<u16>> = None;
        let mut the_power: Option<String> = None;

        loop {
            select! {
                Ok(priorities) = recv(&mut priorities) => { the_priorities = Some(priorities)},
                Ok(power) = recv(&mut power) => { the_power = Some(power)},
                else => { break; }
            }

            let value = match (&the_priorities, the_power.as_deref()) {
                (_, None) => None,
                (_, Some("HARD_OFF")) => Some(Power::HardOff),
                (_, Some("ERROR")) => Some(Power::Error),
                (None, _) => None,
                (Some(priorities), Some("ON")) if priorities.is_empty() => Some(Power::On),
                (Some(priorities), Some("OFF")) if priorities.is_empty() => Some(Power::Off),
                (Some(priorities), _) => {
                    if priorities.contains(&100) {
                        Some(Power::On)
                    } else {
                        Some(Power::Off)
                    }
                }
            };

            if let Some(value) = value {
                send_or_log(&tx, value);
            }
        }
    });

    output.to_rx_pipe()
}
