use anyhow::Result;
use chrono::DateTime;
use chrono::Timelike;
use chrono::Utc;
use chrono_tz::Tz;

use log::*;
use paho_mqtt::Message;
use robotica_node_rust::send_or_discard;
use robotica_node_rust::sources::mqtt::MqttOut;
use robotica_node_rust::sources::mqtt::Subscriptions;
use robotica_node_rust::sources::timer;
use robotica_node_rust::spawn;
use robotica_node_rust::Pipe;
use robotica_node_rust::RxPipe;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::select;

use super::robotica::string_to_power;
use super::robotica::Power;
use super::robotica::RoboticaAutoColor;
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
    light("Brian", "Light", subscriptions, mqtt_out);
    light("Dining", "Light", subscriptions, mqtt_out);
    light("Passage", "Light", subscriptions, mqtt_out);
    light("Twins", "Light", subscriptions, mqtt_out);
    light("Akira", "Light", subscriptions, mqtt_out);
    light("Passage", "Light", subscriptions, mqtt_out);

    device("Brian", "Fan", subscriptions, mqtt_out);
    device("Dining", "TvSwitch", subscriptions, mqtt_out);
}

fn light_google_to_robotica(payload: String, location: &str, device: &str) -> Option<Message> {
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

    let action = if gc.on {
        None
    } else {
        Some("turn_off".to_string())
    };

    let scene = match (location, device) {
        ("Brian", "Light") => "auto".to_string(),
        (_, _) => "default".to_string(),
    };

    let command = RoboticaLightCommand {
        action,
        color: Some(color),
        scene: Some(scene),
    };

    let topic = format!("command/{location}/{device}");
    let payload = serde_json::to_string(&command).unwrap();
    Some(Message::new(topic, payload, 0))
}

fn device_google_to_robotica(payload: String, location: &str, device: &str) -> Option<Message> {
    let d = &mut serde_json::Deserializer::from_str(&payload);
    let gc: Result<GoogleCommand, _> = serde_path_to_error::deserialize(d);

    if let Err(err) = gc {
        error!("device_google_to_robotica: {err}");
        return None;
    }

    let gc = gc.unwrap();
    let action = if gc.on {
        Some("turn_on".to_string())
    } else {
        Some("turn_off".to_string())
    };

    let command = RoboticaDeviceCommand { action };
    let topic = format!("command/{location}/{device}");
    let payload = serde_json::to_string(&command).unwrap();
    Some(Message::new(topic, payload, 0))
}

fn robotica_to_google(power: Power, location: &str, device: &str) -> Message {
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

    let topic = format!("google/{location}/{device}/in");
    let payload = serde_json::to_string(&command).unwrap();
    Message::new(topic, payload, 0)
}

fn timer_to_color(location: &str, _device: &str) -> RoboticaAutoColor {
    let now: DateTime<Utc> = Utc::now();
    let tz: Tz = "Australia/Melbourne".parse().unwrap();
    let local_now = now.with_timezone(&tz);
    let hour = local_now.hour();

    let brightness = if location == "Brian" {
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
        power: Power::On,
        color: RoboticaColorOut {
            hue: 0,
            saturation: 0,
            brightness,
            kelvin,
        },
    }
}

fn color_to_message(color: RoboticaAutoColor, location: &str, device: &str) -> Message {
    let topic = format!("command/{location}/{device}/scene/auto");
    let payload = serde_json::to_string(&color).unwrap();
    Message::new_retained(topic, payload, 0)
}

fn light(location: &str, device: &str, subscriptions: &mut Subscriptions, mqtt_out: &MqttOut) {
    {
        let location = location.to_string();
        let device = device.to_string();
        let topic = format!("google/{location}/{device}/out");
        subscriptions
            .subscribe(&topic)
            .filter_map(move |payload| light_google_to_robotica(payload, &location, &device))
            .publish(mqtt_out);
    }

    {
        let topic = format!("state/{location}/{device}/power");
        let power_str = subscriptions.subscribe(&topic);

        let topic = format!("state/{location}/{device}/priorities");
        let priorities = subscriptions.subscribe(&topic).map(|payload| {
            let list: Vec<u16> = serde_json::from_str(&payload).unwrap();
            list
        });

        let location = location.to_string();
        let device = device.to_string();
        light_power(&priorities, &power_str)
            .map(move |power| robotica_to_google(power, &location, &device))
            .publish(mqtt_out);
    }

    {
        let location1 = location.to_string();
        let device1 = device.to_string();
        let location2 = location.to_string();
        let device2 = device.to_string();
        timer::timer(Duration::from_secs(60))
            .map(move |_| timer_to_color(&location1, &device1))
            .diff()
            .changed()
            .map(move |c| color_to_message(c, &location2, &device2))
            .publish(mqtt_out);
    }
}

fn device(location: &str, device: &str, subscriptions: &mut Subscriptions, mqtt_out: &MqttOut) {
    {
        let location = location.to_string();
        let device = device.to_string();
        let topic = format!("google/{location}/{device}/out");

        subscriptions
            .subscribe(&topic)
            .filter_map(move |payload| device_google_to_robotica(payload, &location, &device))
            .publish(mqtt_out);
    }

    {
        let location = location.to_string();
        let device = device.to_string();
        let topic = format!("state/{location}/{device}/power");

        subscriptions
            .subscribe(&topic)
            .map(string_to_power)
            .map(move |power| robotica_to_google(power, &location, &device))
            .publish(mqtt_out);
    }
}

fn light_power(priorities: &RxPipe<Vec<u16>>, power: &RxPipe<String>) -> RxPipe<Power> {
    let output = Pipe::new();
    let tx = output.get_tx();
    let mut priorities = priorities.subscribe();
    let mut power = power.subscribe();

    spawn(async move {
        let mut the_priorities: Option<Vec<u16>> = None;
        let mut the_power: Option<String> = None;

        loop {
            select! {
                Ok(priorities) = priorities.recv() => { the_priorities = Some(priorities)},
                Ok(power) = power.recv() => { the_power = Some(power)},
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
                send_or_discard(&tx, value);
            }
        }
    });

    output.to_rx_pipe()
}
