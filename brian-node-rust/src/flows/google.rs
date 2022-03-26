use anyhow::Result;
use chrono::DateTime;
use chrono::Timelike;
use chrono::Utc;
use chrono_tz::Tz;

use log::*;
use paho_mqtt::Message;
use robotica_node_rust::{
    filters::ChainGeneric,
    send,
    sources::{
        mqtt::{MqttMessage, Subscriptions},
        timer::timer,
        ChainMqtt,
    },
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::{
    select,
    sync::mpsc::{self, Receiver, Sender},
};

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "UPPERCASE")]
enum Power {
    On,
    Off,
    HardOff,
    Error,
}

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

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RoboticaColorOut {
    hue: u16,
    saturation: u16,
    brightness: u16,
    kelvin: u16,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RoboticaCommand {
    action: Option<String>,
    color: Option<RoboticaColorOut>,
    scene: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RoboticaAutoColor {
    power: Power,
    color: RoboticaColorOut,
}

pub fn start(subscriptions: &mut Subscriptions, mqtt_out: &Sender<MqttMessage>) {
    light("Brian", "Light", subscriptions, mqtt_out);
    light("Dining", "Light", subscriptions, mqtt_out);
    light("Passage", "Light", subscriptions, mqtt_out);
    light("Twins", "Light", subscriptions, mqtt_out);
    light("Akira", "Light", subscriptions, mqtt_out);
    light("Passage", "Light", subscriptions, mqtt_out);

    device("Fan", "Light", subscriptions, mqtt_out);
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

    let command = RoboticaCommand {
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

    let command = RoboticaCommand {
        action,
        scene: None,
        color: None,
    };

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
            online: true,
        },
    };

    let topic = format!("google/{location}/{device}/in");
    let payload = serde_json::to_string(&command).unwrap();
    Message::new(topic, payload, 0)
}

fn timer_to_auto(location: &str, device: &str) -> Message {
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

    let command = RoboticaAutoColor {
        power: Power::On,
        color: RoboticaColorOut {
            hue: 0,
            saturation: 0,
            brightness,
            kelvin,
        },
    };

    let topic = format!("command/{location}/{device}/scene/auto");
    let payload = serde_json::to_string(&command).unwrap();
    Message::new_retained(topic, payload, 0)
}

fn light(
    location: &str,
    device: &str,
    subscriptions: &mut Subscriptions,
    mqtt_out: &Sender<MqttMessage>,
) {
    {
        let location = location.to_string();
        let device = device.to_string();
        let topic = format!("google/{location}/{device}/out");
        subscriptions
            .subscribe(&topic)
            .filter_map(move |payload| light_google_to_robotica(payload, &location, &device))
            .publish(mqtt_out.clone());
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
        power(priorities, power_str)
            .map(move |power| robotica_to_google(power, &location, &device))
            .publish(mqtt_out.clone());
    }

    {
        let location = location.to_string();
        let device = device.to_string();
        timer(Duration::from_secs(60))
            .map(move |_| timer_to_auto(&location, &device))
            .publish(mqtt_out.clone());
    }
}

fn device(
    location: &str,
    device: &str,
    subscriptions: &mut Subscriptions,
    mqtt_out: &Sender<MqttMessage>,
) {
    {
        let location = location.to_string();
        let device = device.to_string();
        let topic = format!("google/{location}/{device}/out");
        subscriptions
            .subscribe(&topic)
            .filter_map(move |payload| device_google_to_robotica(payload, &location, &device))
            .publish(mqtt_out.clone());
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
        power(priorities, power_str)
            .map(move |power| robotica_to_google(power, &location, &device))
            .publish(mqtt_out.clone());
    }
}

fn power(
    mut priorities: mpsc::Receiver<Vec<u16>>,
    mut power: mpsc::Receiver<String>,
) -> Receiver<Power> {
    let (tx, rx) = mpsc::channel(10);
    tokio::spawn(async move {
        let mut the_priorities: Option<Vec<u16>> = None;
        let mut the_power: Option<String> = None;

        loop {
            select! {
                Some(priorities) = priorities.recv() => { the_priorities = Some(priorities)},
                Some(power) = power.recv() => { the_power = Some(power)},
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
                send(&tx, value).await;
            }
        }
    });

    rx
}
