mod http;
mod robotica;

use std::fmt::Display;
use std::time::Duration;

use anyhow::Result;
use robotica::Id;
use robotica_node_rust::entities::create_entity;
use robotica_node_rust::spawn;
use thiserror::Error;
use tokio::select;
use tokio::sync::mpsc;

use robotica::Power;
use robotica_node_rust::sources::mqtt::{Message, MqttOut};
use robotica_node_rust::sources::mqtt::{MqttClient, Subscriptions};
use tokio::time::{sleep_until, Instant};

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();
    color_backtrace::install();

    http::start().await;

    let mut mqtt = MqttClient::new();
    let mqtt_out = mqtt.get_mqtt_out();

    let subscriptions: Subscriptions = setup_pipes(mqtt_out).await;
    mqtt.connect(subscriptions)?;
    mqtt.wait().await;

    Ok(())
}

struct State {
    subscriptions: Subscriptions,
    #[allow(dead_code)]
    mqtt_out: MqttOut,
    message_sink: mpsc::Sender<String>,
}

async fn setup_pipes(mqtt_out: MqttOut) -> Subscriptions {
    let mut subscriptions: Subscriptions = Subscriptions::new();

    let message_sink = create_message_sink(&mut subscriptions, mqtt_out.clone());

    let mut state = State {
        subscriptions,
        mqtt_out,
        message_sink,
    };

    monitor_tesla_doors(&mut state, 1);

    let message_sink_temp = state.message_sink.clone();
    let rx = state
        .subscriptions
        .subscribe_into::<Power>("state/Brian/Light/power");
    tokio::spawn(async move {
        let mut s = rx.subscribe().await;
        loop {
            let msg = s.recv().await;
            if let Ok((Some(prev), current)) = msg {
                let announce = format!("Light power changed from {} to {}", prev, current);

                if let Err(err) = message_sink_temp.send(announce).await {
                    println!("Error sending message: {}", err);
                }
            }
            if let Some(msg) = rx.get().await {
                println!("get: {:?}", msg);
            }
        }
    });

    state.subscriptions
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TeslaDoorState {
    Open,
    Closed,
}

impl Display for TeslaDoorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeslaDoorState::Open => write!(f, "open"),
            TeslaDoorState::Closed => write!(f, "closed"),
        }
    }
}

impl TryFrom<Message> for TeslaDoorState {
    type Error = TeslaStateErr;
    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        let payload: String = msg.try_into()?;
        match payload.as_str() {
            "true" => Ok(TeslaDoorState::Open),
            "false" => Ok(TeslaDoorState::Closed),
            _ => Err(TeslaStateErr::InvalidDoorState(payload)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TeslaUserIsPresent {
    UserPresent,
    UserNotPresent,
}

impl Display for TeslaUserIsPresent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeslaUserIsPresent::UserPresent => write!(f, "user is present"),
            TeslaUserIsPresent::UserNotPresent => write!(f, "user is not present"),
        }
    }
}

impl TryFrom<Message> for TeslaUserIsPresent {
    type Error = TeslaStateErr;
    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        let payload: String = msg.try_into()?;
        match payload.as_str() {
            "true" => Ok(TeslaUserIsPresent::UserPresent),
            "false" => Ok(TeslaUserIsPresent::UserNotPresent),
            _ => Err(TeslaStateErr::InvalidDoorState(payload)),
        }
    }
}

#[derive(Error, Debug)]
enum TeslaStateErr {
    #[error("Invalid door state: {0}")]
    InvalidDoorState(String),

    #[error("Invalid UTF8")]
    Utf8Error(#[from] std::str::Utf8Error),
}

fn monitor_tesla_doors(state: &mut State, car_number: usize) {
    let fo_rx = state
        .subscriptions
        .subscribe_into::<TeslaDoorState>(&format!("teslamate/cars/{car_number}/frunk_open"));
    let to_rx = state
        .subscriptions
        .subscribe_into::<TeslaDoorState>(&format!("teslamate/cars/{car_number}/trunk_open"));
    let do_rx = state
        .subscriptions
        .subscribe_into::<TeslaDoorState>(&format!("teslamate/cars/{car_number}/doors_open"));
    let wo_rx = state
        .subscriptions
        .subscribe_into::<TeslaDoorState>(&format!("teslamate/cars/{car_number}/windows_open"));
    let up_rx = state
        .subscriptions
        .subscribe_into::<TeslaUserIsPresent>(&format!(
            "teslamate/cars/{car_number}/is_user_present"
        ));

    let message_sink = state.message_sink.clone();

    let (tx, rx) = create_entity("tesla_doors");

    spawn(async move {
        let mut fo_s = fo_rx.subscribe().await;
        let mut to_s = to_rx.subscribe().await;
        let mut do_s = do_rx.subscribe().await;
        let mut wo_s = wo_rx.subscribe().await;
        let mut up_s = up_rx.subscribe().await;

        loop {
            select! {
                Ok((_, _)) = fo_s.recv() => {},
                Ok((_, _)) = to_s.recv() => {},
                Ok((_, _)) = do_s.recv() => {},
                Ok((_, _)) = wo_s.recv() => {},
                Ok((_, _)) = up_s.recv() => {},
                else => break,
            };

            let mut open: Vec<&str> = vec![];

            let maybe_up = up_rx.get().await;
            if let Some(TeslaUserIsPresent::UserNotPresent) = maybe_up {
                let maybe_fo = fo_rx.get().await;
                let maybe_to = to_rx.get().await;
                let maybe_do = do_rx.get().await;
                let maybe_wo = wo_rx.get().await;

                println!(
                    "fo: {:?}, to: {:?}, do: {:?}, wo: {:?}, up: {:?}",
                    maybe_fo, maybe_to, maybe_do, maybe_wo, maybe_up
                );

                if let Some(TeslaDoorState::Open) = maybe_fo {
                    open.push("frunk")
                }

                if let Some(TeslaDoorState::Open) = maybe_to {
                    open.push("trunk")
                }

                if let Some(TeslaDoorState::Open) = maybe_do {
                    open.push("doors")
                }

                if let Some(TeslaDoorState::Open) = maybe_wo {
                    open.push("windows")
                }
            } else {
                println!("up: {:?}", maybe_up);
            }

            println!("open: {:?}", open);
            tx.send(open).await;
        }
    });

    let (tx2, rx2) = create_entity("tesla_doors_delayed");
    spawn(async move {
        let mut state = DelayState::Idle;
        let duration = Duration::from_secs(60);
        let mut s = rx.subscribe().await;

        loop {
            select! {
                Ok((_, v)) = s.recv() => {
                    println!("delay received: {:?}", v);
                    let active_value = !v.is_empty();
                    match (active_value, &state) {
                        (false, _) => {
                            state = DelayState::Idle;
                            tx2.send(v).await;
                        },
                        (true, DelayState::Idle) => {
                            state = DelayState::Delaying(Instant::now() + duration, v);
                        },
                        (true, DelayState::Delaying(instant, _)) => {
                            state = DelayState::Delaying(*instant, v);
                        },
                        (true, DelayState::NoDelay) => {
                            tx2.send(v).await;
                        },
                    }

                },
                Some(()) = maybe_sleep_until(&state) => {
                    println!("delay timer");
                    if let DelayState::Delaying(_, v) = state {
                        tx2.send(v).await;
                    }
                    state = DelayState::NoDelay;
                },
                else => { break; }
            }
        }
    });

    spawn(async move {
        let mut s = rx2.subscribe().await;
        while let Ok((prev, open)) = s.recv().await {
            println!("out received: {:?} {:?}", prev, open);
            if prev.is_none() {
                continue;
            }
            let msg = if open.is_empty() {
                "The Tesla is secure".to_string()
            } else {
                format!("The Tesla {} are open", open.join(", "))
            };
            if let Err(err) = message_sink.send(msg).await {
                println!("Error sending message: {}", err);
            }
        }
    });
}

fn create_message_sink(
    subscriptions: &mut Subscriptions,
    mqtt_out: MqttOut,
) -> mpsc::Sender<String> {
    let gate_topic = Id::new("Brian", "Messages").get_state_topic("power");
    let gate_in = subscriptions.subscribe_into::<Power>(&gate_topic);

    let (tx, mut rx) = mpsc::channel::<String>(100);
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("{}", msg);

            if let Some(Power::On) = gate_in.get().await {
                let msg = robotica::string_to_message(&msg, "Brian");
                mqtt_out.send(msg).await;
            }
        }
    });
    tx
}

enum DelayState<T> {
    Idle,
    Delaying(Instant, T),
    NoDelay,
}

async fn maybe_sleep_until<T>(state: &DelayState<T>) -> Option<()> {
    if let DelayState::Delaying(instant, _) = state {
        sleep_until(*instant).await;
        Some(())
    } else {
        None
    }
}
