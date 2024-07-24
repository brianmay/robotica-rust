//! Main entry point for the application.

#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![allow(clippy::to_string_trait_impl)]

mod amber;
mod config;
mod ha;
mod hdmi;
mod influxdb;
mod lights;
mod logging;
mod metrics;
mod robotica;
mod rooms;
mod tesla;

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use chrono::{Local, TimeZone};
use lights::{run_auto_light, run_split_light, Scene, SceneMap, SplitPowerColor};
use robotica_backend::devices::lifx::{DeviceConfig, DiscoverConfig};
use robotica_backend::devices::{fake_switch, lifx};
use robotica_backend::pipes::{stateful, stateless, Subscriber};
use robotica_backend::scheduling::calendar::{CalendarEntry, StartEnd};
use robotica_backend::scheduling::executor::executor;
use robotica_backend::scheduling::sequencer::Sequence;
use robotica_backend::services::persistent_state::PersistentStateDatabase;
use robotica_backend::spawn;
use robotica_common::mqtt::{Json, MqttMessage, QoS, Retain};
use robotica_common::robotica::audio::MessagePriority;
use robotica_common::robotica::commands::Command;
use robotica_common::robotica::lights::{LightCommand, PowerColor, PowerState, SceneName, State};
use robotica_common::robotica::message::{Audience, Message};
use robotica_common::robotica::tasks::{Payload, Task};
use robotica_common::scheduler::Importance;
use robotica_common::shelly;
use robotica_common::zigbee2mqtt::{Door, DoorState};
use tracing::{debug, error, info};

use crate::amber::hot_water;

use robotica_backend::services::http;
use robotica_backend::services::mqtt::{mqtt_channel, run_client, SendOptions, Subscriptions};
use robotica_backend::services::mqtt::{MqttRx, MqttTx};

#[allow(unreachable_code)]
#[tokio::main]
async fn main() -> Result<()> {
    color_backtrace::install();
    let started = stateless::Started::new();

    let env = config::Environment::load().unwrap_or_else(|e| {
        panic!("Error loading environment: {e}");
    });

    let config = env.config().unwrap_or_else(|e| {
        panic!("Error loading config: {e}");
    });

    let _guard = logging::init_tracing_subscriber(&config.logging).unwrap_or_else(|e| {
        panic!("Error initializing tracing subscriber: {e}");
    });

    info!("Starting brian-backend",);

    let (mqtt, mqtt_rx) = mqtt_channel();
    let subscriptions: Subscriptions = Subscriptions::new();
    let message_sink = ha::create_message_sink(&mqtt);
    let persistent_state_database = PersistentStateDatabase::new(&config.persistent_state)
        .unwrap_or_else(|e| {
            panic!("Error getting persistent state loader: {e}");
        });

    let state = InitState {
        subscriptions,
        mqtt,
        message_sink,
        persistent_state_database,
    };

    let postgres = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("../migrations").run(&postgres).await?;

    setup_pipes(state, mqtt_rx, config, postgres).await;

    started.notify();

    loop {
        debug!("I haven't crashed yet!");
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
    }

    Ok(())
}

/// Global state for initialization.
pub struct InitState {
    /// Subscriptions to MQTT topics.
    pub subscriptions: Subscriptions,

    /// MQTT client.
    #[allow(dead_code)]
    pub mqtt: MqttTx,

    /// Message sink for sending verbal messages.
    pub message_sink: stateless::Sender<Message>,

    /// Persistent state database.
    pub persistent_state_database: PersistentStateDatabase,
}

fn calendar_to_sequence(
    event: CalendarEntry,
    timezone: Local,
    audience: &Audience,
) -> Option<Sequence> {
    let (start_time, end_time) = calendar_start_top_times(&event, timezone).or_else(|| {
        error!("Error getting start/stop times from calendar event {event:?}");
        None
    })?;

    let payload = Message::new(
        "Calendar Event",
        &event.summary,
        MessagePriority::Low,
        audience.clone(),
    );

    let tasks = match event.start_end {
        StartEnd::Date(_, _) => vec![],
        StartEnd::DateTime(_, _) => vec![Task {
            title: format!("Tell everyone {}", event.summary),
            payload: Payload::Command(Command::Message(payload)),
            qos: QoS::ExactlyOnce,
            retain: Retain::NoRetain,
            topics: ["ha/event/message".to_string()].to_vec(),
        }],
    };

    #[allow(deprecated)]
    Some(Sequence {
        title: event.summary.clone(),
        id: event.uid,
        importance: Importance::High,
        sequence_name: event.summary,
        start_time,
        end_time,
        latest_time: end_time,
        tasks,
        mark: None,
        if_cond: None,
        classifications: None,
        options: None,
        zero_time: true,
        repeat_number: 1,
        status: None,

        // These fields are set by executor.
        // It doesn't matter if we get then wrong here.
        // Insert dummy values for now.
        schedule_date: chrono::Utc::now().date_naive(),
        duration: Duration::ZERO,
    })
}

fn calendar_start_top_times(
    event: &CalendarEntry,
    timezone: Local,
) -> Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> {
    let (start_time, end_time) = match event.start_end {
        StartEnd::Date(start, stop) => {
            let start = start.and_hms_opt(0, 0, 0)?;
            let stop = stop.and_hms_opt(0, 0, 0)?;
            let start = timezone
                .from_local_datetime(&start)
                .single()?
                .with_timezone(&chrono::Utc);
            let stop = timezone
                .from_local_datetime(&stop)
                .single()?
                .with_timezone(&chrono::Utc);
            (start, stop)
        }
        StartEnd::DateTime(start, stop) => (start, stop),
    };
    Some((start_time, end_time))
}

async fn setup_pipes(
    mut state: InitState,
    mqtt_rx: MqttRx,
    config: config::Config,
    postgres: sqlx::PgPool,
) {
    let (prices, usage) = amber::run(config.amber).unwrap_or_else(|e| {
        panic!("Error running amber: {e}");
    });
    amber::logging::log_prices(prices.clone(), &config.influxdb);
    amber::logging::log_usage(usage, &config.influxdb);

    monitor_hot_water(&mut state, &prices);

    monitor_bathroom_door(&mut state);

    monitor_teslas(&config.teslas, &mut state, &postgres, &prices);

    let rooms = rooms::get();
    http::run(state.mqtt.clone(), rooms, config.http, postgres.clone())
        .await
        .unwrap_or_else(|e| panic!("Error running http server: {e}"));

    hdmi::run(&mut state, "Dining", "TV", "hdmi.pri:8000");

    let mut raw_metrics: Vec<metrics::RawMetric> = vec![];
    for metric in config.metrics {
        let raw: Vec<metrics::RawMetric> = metric.into();
        raw_metrics.extend(raw);
    }
    for metric in raw_metrics {
        metric.monitor(&mut state.subscriptions, &config.influxdb);
    }

    executor(
        &mut state.subscriptions,
        state.mqtt.clone(),
        config.executor,
        Box::new(move |event, timezone| {
            calendar_to_sequence(event, timezone, &Audience::new("everyone"))
        }),
        Local,
    )
    .unwrap_or_else(|err| {
        panic!("Failed to start executor: {err}");
    });

    fake_switch(&mut state, "Brian/Night");

    setup_lights(&mut state, &config.lifx, &config.lights, &config.strips).await;

    run_client(state.subscriptions, mqtt_rx, config.mqtt).unwrap_or_else(|e| {
        panic!("Error running mqtt client: {e}");
    });
}

fn monitor_bathroom_door(state: &mut InitState) {
    let bathroom_door: stateful::Receiver<DoorState> = state
        .subscriptions
        .subscribe_into_stateful::<Json<Door>>("zigbee2mqtt/Bathroom/door")
        .map(|(_, json)| json.into())
        .rate_limit("Bathroom Door Rate Limited", Duration::from_secs(30));

    let mqtt = state.mqtt.clone();
    let message_sink = state.message_sink.clone();
    spawn(async move {
        let mut s = bathroom_door.subscribe().await;

        while let Ok((old, door)) = s.recv_old_new().await {
            if old.is_some() {
                info!("Bathroom door state: {door:?}");
                let action = match door {
                    DoorState::Open => LightCommand::TurnOff,
                    DoorState::Closed => LightCommand::TurnOn {
                        scene: SceneName::new("busy"),
                    },
                };
                let command = Command::Light(action);
                let message = match door {
                    DoorState::Open => "The bathroom is vacant",
                    DoorState::Closed => "The bathroom is occupied",
                };
                mqtt.try_serialize_send(
                    "command/Passage/Light/split/bathroom",
                    &Json(command),
                    Retain::NoRetain,
                    QoS::ExactlyOnce,
                );
                message_sink.try_send(Message::new(
                    "Bathroom Door",
                    message,
                    MessagePriority::DaytimeOnly,
                    Audience::new("everyone"),
                ));
            }
        }
    });
}

fn monitor_hot_water(
    state: &mut InitState,
    prices: &stateful::Receiver<std::sync::Arc<amber::Prices>>,
) {
    let is_on = state
        .subscriptions
        .subscribe_into_stateful::<Json<shelly::SwitchStatus>>("hotwater/status/switch:0")
        .map(|(_, json)| json.0.output);

    let rules = state
        .subscriptions
        .subscribe_into_stateless::<Json<amber::rules::RuleSet<amber::hot_water::Request>>>(
            "robotica/command/hot_water/rules",
        );

    let mqtt_clone = state.mqtt.clone();
    let hot_water_state = amber::hot_water::run(
        &state.persistent_state_database,
        prices.clone(),
        is_on,
        rules,
    );
    hot_water_state.clone().send_to_mqtt_json(
        &state.mqtt,
        "robotica/state/hot_water/amber",
        &SendOptions::new(),
    );
    let hot_water_request = hot_water_state.map(|(_, state)| state.get_result());

    let message_sink = state.message_sink.clone();
    hot_water_request.for_each(move |(old, current)| {
        let command = match current {
            hot_water::Request::Heat => shelly::SwitchCommand::On(None),
            hot_water::Request::DoNotHeat => shelly::SwitchCommand::Off(None),
        };
        info!("Setting hot water to {:?}", command);
        let msg = MqttMessage::new(
            "hotwater/command/switch:0",
            command,
            Retain::NoRetain,
            QoS::ExactlyOnce,
        );
        mqtt_clone.try_send(msg);

        // Don't announce when first starting up.
        if old.is_some() {
            let message = match current {
                hot_water::Request::Heat => "Turning hot water on",
                hot_water::Request::DoNotHeat => "Turning hot water off",
            };
            message_sink.try_send(Message::new(
                "Hot Water",
                message,
                MessagePriority::DaytimeOnly,
                Audience::new("everyone"),
            ));
        }
    });
}

fn monitor_teslas(
    teslas: &[tesla::Config],
    state: &mut InitState,
    postgres: &sqlx::Pool<sqlx::Postgres>,
    prices: &stateful::Receiver<std::sync::Arc<amber::Prices>>,
) {
    let token = tesla::token::run(state).unwrap_or_else(|e| {
        panic!("Error running tesla token generator: {e}");
    });

    for tesla in teslas {
        let receivers = tesla::Receivers::new(tesla, state);

        let locations = tesla::monitor_teslamate_location::monitor(
            tesla,
            receivers.location.clone(),
            postgres.clone(),
        );

        locations.messages.send_to(&state.message_sink);
        locations.location_message.send_to_mqtt_json(
            &state.mqtt,
            format!(
                "state/Tesla/{id}/Locations",
                id = tesla.teslamate_id.to_string()
            ),
            &SendOptions::new(),
        );

        let charge_state = amber::car::run(
            &state.persistent_state_database,
            tesla.teslamate_id,
            prices.clone(),
            receivers.battery_level.clone(),
            receivers.min_charge_tomorrow.clone(),
            receivers.is_charging.clone(),
            receivers.rules.clone(),
        );
        charge_state.clone().send_to_mqtt_json(
            &state.mqtt,
            format!(
                "robotica/state/tesla/{id}/amber",
                id = tesla.teslamate_id.to_string()
            ),
            &SendOptions::new(),
        );
        let charge_request = charge_state.map(|(_, state)| state.get_result());

        let monitor_charging_receivers = tesla::monitor_charging::Inputs::from_receivers(
            &receivers,
            charge_request,
            locations.is_home,
        );
        let outputs = tesla::monitor_charging::monitor_charging(
            &state.persistent_state_database,
            tesla,
            monitor_charging_receivers,
        )
        .unwrap_or_else(|e| {
            panic!("Error running tesla charging monitor: {e}");
        });

        outputs.auto_charge.send_to_mqtt_string(
            &state.mqtt,
            format!(
                "state/Tesla/{id}/AutoCharge/power",
                id = tesla.teslamate_id.to_string()
            ),
            &SendOptions::new(),
        );

        let should_plugin_stream = tesla::monitor_location::monitor(
            tesla,
            state.message_sink.clone(),
            locations.location,
            outputs.charging_information,
        );

        tesla::command_processor::run(tesla, outputs.commands, token.clone())
            .send_to(&state.message_sink);

        let monitor_doors_receivers =
            tesla::monitor_doors::MonitorInputs::from_receivers(&receivers);

        tesla::monitor_doors::monitor(tesla, monitor_doors_receivers).send_to(&state.message_sink);
        tesla::plug_in_reminder::plug_in_reminder(tesla, should_plugin_stream)
            .send_to(&state.message_sink);
    }
}

fn fake_switch(state: &mut InitState, topic_substr: &str) {
    let topic_substr: String = topic_substr.into();
    let topic = format!("command/{topic_substr}");
    let rx = state
        .subscriptions
        .subscribe_into_stateless::<Json<Command>>(&topic);

    let topic = format!("state/{topic_substr}/power");
    fake_switch::run(rx).send_to_mqtt_string(&state.mqtt, topic, &SendOptions::new());
}

async fn setup_lights(
    state: &mut InitState,
    lifx: &config::LifxConfig,
    lights: &[config::LightConfig],
    strips: &[config::StripConfig],
) {
    let lifx_config = DiscoverConfig {
        broadcast: lifx.broadcast.clone(),
        poll_time: std::time::Duration::from_secs(10),
        device_timeout: std::time::Duration::from_secs(45),
        api_timeout: std::time::Duration::from_secs(1),
        num_retries: 3,
    };
    let discover = lifx::discover(lifx_config)
        .await
        .unwrap_or_else(|e| panic!("Error discovering lifx devices: {e}"));

    let shared = lights::get_default_scenes();

    for light_config in lights {
        auto_light(state, &discover, &shared, light_config);
    }

    for strip_config in strips {
        strip_light(state, &discover, &shared, strip_config);
    }
}

fn auto_light(
    init_state: &mut InitState,
    discover: &stateless::Receiver<lifx::Device>,
    shared: &SceneMap,
    config: &config::LightConfig,
) {
    let topic_substr = &config.topic_substr;

    let inputs = lights::Inputs {
        commands: init_state
            .subscriptions
            .subscribe_into_stateless::<Json<Command>>(format!("command/{topic_substr}")),
    };

    let hash_map: HashMap<SceneName, Scene> = config
        .scenes
        .iter()
        .map(|(name, scene_config)| {
            let scene = scene_config.get_scene(init_state, name.clone());
            (name.clone(), scene)
        })
        .collect();

    let auto_scene = Scene::new(
        init_state
            .subscriptions
            .subscribe_into_stateful::<Json<PowerColor>>(format!("command/{topic_substr}/auto"))
            .map(|(_, Json(pc))| pc),
        SceneName::new("auto"),
    );

    let scene_map = {
        let mut scene_map = SceneMap::new(HashMap::new());
        scene_map.merge(shared.clone());
        scene_map.merge(SceneMap::new(hash_map));
        scene_map.insert(SceneName::new("auto"), auto_scene);
        scene_map
    };

    let lights::Outputs { pc, scene } = run_auto_light(
        inputs,
        &init_state.persistent_state_database,
        scene_map,
        config.flash_color.clone(),
        topic_substr,
    );

    scene.send_to_mqtt_string(
        &init_state.mqtt,
        format!("state/{topic_substr}/scene"),
        &SendOptions::new(),
    );

    send_to_device(&config.device, topic_substr, pc, discover, init_state);
}

fn split_light(
    init_state: &mut InitState,
    shared: &SceneMap,
    topic_substr: &str,
    scenes: &HashMap<SceneName, config::LightSceneConfig>,
    flash_color: &PowerColor,
    priority: usize,
) -> stateful::Receiver<SplitPowerColor> {
    let inputs = lights::Inputs {
        commands: init_state
            .subscriptions
            .subscribe_into_stateless::<Json<Command>>(format!("command/{topic_substr}")),
    };

    let hash_map: HashMap<SceneName, Scene> = scenes
        .iter()
        .map(|(name, scene_config)| {
            let scene = scene_config.get_scene(init_state, name.clone());
            (name.clone(), scene)
        })
        .collect();

    let auto_scene = Scene::new(
        init_state
            .subscriptions
            .subscribe_into_stateful::<Json<PowerColor>>(format!("command/{topic_substr}/auto"))
            .map(|(_, Json(pc))| pc),
        SceneName::new("auto"),
    );

    let scene_map = {
        let mut scene_map = SceneMap::new(HashMap::new());
        scene_map.merge(shared.clone());
        scene_map.merge(SceneMap::new(hash_map));
        scene_map.insert(SceneName::new("auto"), auto_scene);
        scene_map
    };

    let lights::SplitOutputs { spc, scene } = run_split_light(
        inputs,
        &init_state.persistent_state_database,
        scene_map,
        flash_color.clone(),
        topic_substr,
        priority,
    );

    scene.send_to_mqtt_string(
        &init_state.mqtt,
        format!("state/{topic_substr}/scene"),
        &SendOptions::new(),
    );

    spc
}

fn strip_light(
    init_state: &mut InitState,
    discover: &stateless::Receiver<lifx::Device>,
    shared: &SceneMap,
    // topic_substr: &str,
    config: &config::StripConfig,
) {
    let (combined_tx, combined_rx) = stateful::create_pipe(&config.topic_substr);
    let topic_substr = &config.topic_substr;

    for (priority, split) in config.splits.iter().enumerate() {
        let name = &split.name;
        let topic_substr = if name == "all" {
            topic_substr.to_string()
        } else {
            format!("{topic_substr}/split/{name}")
        };
        split_light(
            init_state,
            shared,
            &topic_substr,
            &split.scenes,
            &split.flash_color,
            priority,
        )
        .send_to(&combined_tx);
    }

    let splits: Vec<_> = config
        .splits
        .iter()
        .map(|split| lights::SplitLightConfig {
            begin: split.begin,
            number: split.number,
        })
        .collect();

    let merge_config = lights::MergeLightConfig {
        splits,
        number_of_lights: config.number_of_lights,
    };

    let pc = lights::run_merge_light(combined_rx, topic_substr, merge_config);

    send_to_device(&config.device, topic_substr, pc, discover, init_state);
}

fn send_to_device(
    device: &config::LightDeviceConfig,
    topic_substr: &str,
    pc: stateful::Receiver<PowerColor>,
    discover: &stateless::Receiver<lifx::Device>,
    init_state: &InitState,
) {
    let output = match device {
        config::LightDeviceConfig::Lifx { lifx_id } => lifx::device_entity(
            pc,
            *lifx_id,
            discover,
            DeviceConfig::default().set_multiple_zones(true),
        ),
        config::LightDeviceConfig::Debug { lifx_id } => {
            let lifx_id = *lifx_id;
            pc.map(move |(_, pc)| {
                info!("Debug {lifx_id}: {pc:?}");
                State::Online(pc)
            })
        }
    };

    output.clone().send_to_mqtt_json(
        &init_state.mqtt,
        format!("state/{topic_substr}/status"),
        &SendOptions::new(),
    );

    output
        .map(|(_, pc)| match pc {
            State::Online(PowerColor::On(..)) => PowerState::On,
            State::Online(PowerColor::Off) => PowerState::Off,
            State::Offline => PowerState::Offline,
        })
        .send_to_mqtt_json(
            &init_state.mqtt,
            format!("state/{topic_substr}/power"),
            &SendOptions::new(),
        );
}
