//! Main entry point for the application.

#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![allow(clippy::to_string_trait_impl)]

mod amber;
mod car;
mod config;
mod ha;
mod hdmi;
mod influxdb;
mod lights;
mod logging;
mod metrics;
mod open_epaper_link;
mod robotica;
mod tesla;

use std::collections::HashMap;
use std::time::Duration;

use amber::car::ChargeRequest;
use amber::rules;
use anyhow::Result;
use chrono::{Local, TimeZone};
use lights::{run_auto_light, run_split_light, Scene, SceneMap, SplitPowerColor};
use robotica_common::mqtt::{Json, MqttMessage, Parsed, QoS, Retain};
use robotica_common::robotica::audio::MessagePriority;
use robotica_common::robotica::commands::Command;
use robotica_common::robotica::entities::Id;
use robotica_common::robotica::lights::{LightCommand, PowerColor, PowerState, SceneName, State};
use robotica_common::robotica::message::{Audience, Message};
use robotica_common::robotica::tasks::{Payload, Task};
use robotica_common::scheduler::Importance;
use robotica_common::shelly;
use robotica_common::zigbee2mqtt::{Door, DoorState};
use robotica_tokio::devices::lifx::{DeviceConfig, DiscoverConfig};
use robotica_tokio::devices::occupancy::{self, OccupiedState};
use robotica_tokio::devices::presence_tracker::{is_any_presence_in_room, PresenceTrackerValue};
use robotica_tokio::devices::{fake_switch, lifx, presence_tracker};
use robotica_tokio::pipes::{stateful, stateless, Subscriber};
use robotica_tokio::scheduling::calendar::{CalendarEntry, StartEnd};
use robotica_tokio::scheduling::executor::executor;
use robotica_tokio::scheduling::sequencer::Sequence;
use robotica_tokio::services::persistent_state::PersistentStateDatabase;
use robotica_tokio::spawn;
use tracing::{debug, error, info, instrument, span};

use crate::amber::hot_water;
use crate::lights::{auto_brightness_level, auto_light_color, auto_temperature_level};

use robotica_tokio::services::http;
use robotica_tokio::services::mqtt::{mqtt_channel, run_client, SendOptions, Subscriptions};
use robotica_tokio::services::mqtt::{MqttRx, MqttTx};

#[allow(unreachable_code)]
#[tokio::main]
async fn main() -> Result<()> {
    color_backtrace::install();
    if let Err(e) = rustls::crypto::aws_lc_rs::default_provider().install_default() {
        eprintln!("Failed to install rustls crypto provider: {e:?}");
        std::process::exit(1);
    }
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

    info!("Starting robotica-backend",);

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

#[allow(clippy::too_many_lines)]
async fn setup_pipes(
    mut state: InitState,
    mqtt_rx: MqttRx,
    config: config::Config,
    postgres: sqlx::PgPool,
) {
    if let Some(amber_config) = config.amber {
        let (prices, usage) =
            amber::run(&Id::new("amber_account"), amber_config).unwrap_or_else(|e| {
                panic!("Error running amber: {e}");
            });
        amber::logging::log_prices(prices.clone(), &config.influxdb);
        amber::logging::log_usage(usage, &config.influxdb);

        if let Some(hot_water) = config.hot_water {
            monitor_hot_water(&mut state, hot_water, &prices);
        }

        monitor_cars(&config.cars, &mut state, &postgres, &prices);
    } else {
        info!("No amber configuration found; skipping hot water and car monitoring");
    }

    monitor_bathroom_door(&mut state);

    let presence_trackers: Vec<stateful::Receiver<PresenceTrackerValue>> = config
        .presence_trackers
        .into_iter()
        .map(|tracker| {
            // println!("presence {} {}", tracker.config.id, tracker.topic);
            let subscription = state.subscriptions.subscribe_into_stateless(tracker.topic);
            presence_tracker::run(tracker.config, subscription)
        })
        .collect();

    // presence_trackers[0].clone().for_each(|(_, present)| {
    //     error!("Is Brian present? {present:#?}");
    // });

    let occupancy_sensors: HashMap<String, stateful::Receiver<OccupiedState>> = config
        .occupancy_sensors
        .into_iter()
        .map(|room| {
            let rx = occupancy::subscribe(&room.config, &mut state.subscriptions);
            (room.room, rx)
        })
        .collect();

    // is_any_presence_in_room("brian", presence_trackers.clone()).for_each(|(_, present)| {
    //     error!("Is anyone present in brian? {present}");
    // });
    // auto_brightness_level().for_each(|(_, level)| {
    //     error!("Auto brightness level: {level}");
    // });
    // auto_temperature_level().for_each(|(_, level)| {
    //     error!("Auto temperature level: {level}");
    // });

    if let Some(http_config) = config.http {
        http::run(state.mqtt.clone(), http_config, postgres.clone())
            .await
            .unwrap_or_else(|e| panic!("Error running http server: {e}"));
    }

    hdmi::run(&mut state, "Dining", "TV", "hdmi.pri:8000");

    let mut raw_metrics: Vec<metrics::RawMetric> = vec![];
    for metric in config.metrics {
        let raw: Vec<metrics::RawMetric> = metric.into();
        raw_metrics.extend(raw);
    }
    for metric in raw_metrics {
        metric.monitor(&mut state.subscriptions, &config.influxdb);
    }

    if let Some(executor_config) = config.executor {
        executor(
            &mut state.subscriptions,
            state.mqtt.clone(),
            executor_config,
            Box::new(move |event, timezone| {
                calendar_to_sequence(event, timezone, &Audience::new("everyone"))
            }),
            Local,
        )
        .unwrap_or_else(|err| {
            panic!("Failed to start executor: {err}");
        });
    }

    let night_mode_for_room: HashMap<String, stateful::Receiver<bool>> = config
        .night_mode
        .into_iter()
        .map(|mode| {
            let rx = fake_switch(&mut state, &mode.topic);
            (mode.room, rx)
        })
        .collect();

    if let Some(lifx_config) = &config.lifx {
        let shared = SharedAutoLight {
            brightness: auto_brightness_level(),
            temperature: auto_temperature_level(),
            night_mode_for_room,
            presence_trackers,
            occupancy_sensors,
        };

        setup_lights(
            &mut state,
            lifx_config,
            &config.lights,
            &config.strips,
            &shared,
        )
        .await;
    } else {
        info!("No lifx configuration found; skipping light setup");
    }

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
                    "robotica/command/Passage/Light/split/bathroom",
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

#[allow(clippy::needless_pass_by_value)]
fn monitor_hot_water(
    state: &mut InitState,
    hot_water: config::HotWaterConfig,
    prices: &stateful::Receiver<std::sync::Arc<amber::Prices>>,
) {
    let id = hot_water.id;

    let span = tracing::info_span!("Hot Water");
    let _guard = span.enter();

    let is_on = state
        .subscriptions
        .subscribe_into_stateful::<Json<shelly::SwitchStatus>>("hotwater/status/switch:0")
        .map(|(_, json)| json.0.output);

    let rules = state
        .subscriptions
        .subscribe_into_stateless::<Json<amber::rules::RuleSet<amber::hot_water::Request>>>(
            id.get_command_topic("amber_rules"),
        );

    let mqtt_clone = state.mqtt.clone();
    let hot_water_state = amber::hot_water::run(
        &id,
        &state.persistent_state_database,
        prices.clone(),
        is_on,
        rules,
    );
    hot_water_state.clone().send_to_mqtt_json(
        &state.mqtt,
        id.get_state_topic("amber"),
        &SendOptions::new(),
    );
    let hot_water_request = hot_water_state
        .map(|(_, state)| state.get_result())
        .rate_limit("amber/hot_water/ratelimit", Duration::from_secs(300));

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

fn monitor_cars(
    cars: &[car::Config],
    state: &mut InitState,
    postgres: &sqlx::Pool<sqlx::Postgres>,
    prices: &stateful::Receiver<std::sync::Arc<amber::Prices>>,
) {
    let id = Id::new("tesla_account");
    let token = tesla::token::run(&id, state).unwrap_or_else(|e| {
        panic!("Error running tesla token generator: {e}");
    });

    let teslas = cars.iter().filter_map(|car| match car.make {
        car::MakeConfig::Tesla(ref tesla) => Some((car, tesla)),
        car::MakeConfig::Unknown => None,
    });

    for (car, tesla) in teslas {
        monitor_tesla(car, tesla, state, postgres, prices, &token);
    }
}

#[instrument(fields(id=%car.id), skip_all)]
fn monitor_tesla(
    car: &car::Config,
    tesla: &tesla::Config,
    state: &mut InitState,
    postgres: &sqlx::Pool<sqlx::Postgres>,
    prices: &stateful::Receiver<std::sync::Arc<amber::Prices>>,
    token: &stateless::Receiver<std::sync::Arc<robotica_tokio::services::tesla::api::Token>>,
) {
    let auto_charge = state
        .subscriptions
        .subscribe_into_stateless::<Json<Command>>(car.id.get_command_topic("auto_charge"));

    let min_charge_tomorrow = state
        .subscriptions
        .subscribe_into_stateless::<Parsed<u8>>(car.id.get_command_topic("min_charge_tomorrow"));

    let rules = state
        .subscriptions
        .subscribe_into_stateless::<Json<rules::RuleSet<ChargeRequest>>>(
            car.id.get_command_topic("rules"),
        );

    let receivers = tesla::Receivers::new(tesla, state);

    let locations = tesla::monitor_teslamate_location::monitor(
        car,
        receivers.location.clone(),
        postgres.clone(),
    );

    locations.messages.send_to(&state.message_sink);
    locations.location_message.send_to_mqtt_json(
        &state.mqtt,
        car.id.get_state_topic("locations"),
        &SendOptions::new(),
    );

    let charge_state = amber::car::run(
        car,
        &state.persistent_state_database,
        prices.clone(),
        receivers.battery_level.clone(),
        min_charge_tomorrow,
        receivers.is_charging.clone(),
        rules,
    );
    charge_state.clone().send_to_mqtt_json(
        &state.mqtt,
        car.id.get_state_topic("amber"),
        &SendOptions::new(),
    );

    if let Some(display) = &car.oel_display {
        open_epaper_link::output_location(&car.name, display.clone(), locations.location.clone());
    }

    let charge_request = charge_state.map(|(_, state)| state.get_result());

    let monitor_charging_receivers = tesla::monitor_charging::Inputs::from_receivers(
        &receivers,
        charge_request,
        auto_charge,
        locations.is_home,
    );
    let outputs = tesla::monitor_charging::monitor_charging(
        &state.persistent_state_database,
        car,
        monitor_charging_receivers,
    )
    .unwrap_or_else(|e| {
        panic!("Error running tesla charging monitor: {e}");
    });

    outputs.auto_charge.send_to_mqtt_string(
        &state.mqtt,
        car.id.get_state_topic("auto_charge/power"),
        &SendOptions::new(),
    );

    let should_plugin_stream = tesla::monitor_location::monitor(
        car,
        state.message_sink.clone(),
        locations.location,
        outputs.charging_information,
    );

    tesla::command_processor::run(car, tesla, outputs.commands, token.clone())
        .send_to(&state.message_sink);

    let monitor_doors_receivers = tesla::monitor_doors::MonitorInputs::from_receivers(&receivers);

    tesla::monitor_doors::monitor(car, monitor_doors_receivers).send_to(&state.message_sink);
    tesla::plug_in_reminder::plug_in_reminder(car, should_plugin_stream)
        .send_to(&state.message_sink);
}

#[must_use]
fn fake_switch(state: &mut InitState, topic_substr: &str) -> stateful::Receiver<bool> {
    let topic_substr: String = topic_substr.into();
    let topic = format!("robotica/command/{topic_substr}");
    let rx = state
        .subscriptions
        .subscribe_into_stateless::<Json<Command>>(&topic);

    let topic = format!("robotica/state/{topic_substr}/power");
    let rx = fake_switch::run(rx);
    rx.clone()
        .send_to_mqtt_string(&state.mqtt, topic, &SendOptions::new());
    rx.map(|(_, power)| power.is_on())
}

async fn setup_lights(
    state: &mut InitState,
    lifx: &config::LifxConfig,
    lights: &[config::LightConfig],
    strips: &[config::StripConfig],
    shared: &SharedAutoLight,
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

    let shared_scenes = lights::get_default_scenes();

    for light_config in lights {
        auto_light(state, &discover, &shared_scenes, shared, light_config);
    }

    for strip_config in strips {
        strip_light(state, &discover, &shared_scenes, shared, strip_config);
    }
}

struct SharedAutoLight {
    brightness: stateful::Receiver<f32>,
    temperature: stateful::Receiver<u16>,
    night_mode_for_room: HashMap<String, stateful::Receiver<bool>>,
    presence_trackers: Vec<stateful::Receiver<PresenceTrackerValue>>,
    occupancy_sensors: HashMap<String, stateful::Receiver<OccupiedState>>,
}

impl SharedAutoLight {
    fn get_auto_scene(&self, room: &str, fixed_brightness: Option<f32>) -> Scene {
        let brightness = fixed_brightness.map_or_else(
            || self.brightness.clone(),
            |level| {
                stateful::static_entity(
                    level,
                    format!("auto_brightness_for_room_{room}_fixed_{level}"),
                )
            },
        );

        let night_mode = self
            .night_mode_for_room
            .get(room)
            .cloned()
            .unwrap_or_else(|| {
                stateful::static_entity(false, format!("night_mode_for_room_{room}_default"))
            });

        let presence = is_any_presence_in_room(room, self.presence_trackers.clone());

        let occupied = self
            .occupancy_sensors
            .get(room)
            .cloned()
            .unwrap_or_else(|| {
                stateful::static_entity(
                    OccupiedState::Vacant,
                    format!("occupied_for_room_{room}_default"),
                )
            });

        let rx = auto_light_color(
            brightness,
            self.temperature.clone(),
            night_mode,
            presence,
            occupied,
        );

        Scene::new(rx, SceneName::new("auto"))
    }
}

fn auto_light(
    init_state: &mut InitState,
    discover: &stateless::Receiver<lifx::Device>,
    shared_scenes: &SceneMap,
    shared: &SharedAutoLight,
    config: &config::LightConfig,
) {
    let inputs = lights::Inputs {
        commands: init_state
            .subscriptions
            .subscribe_into_stateless::<Json<Command>>(config.id.get_command_topic("")),
    };

    let hash_map: HashMap<SceneName, Scene> = config
        .scenes
        .iter()
        .map(|(name, scene_config)| {
            let scene = scene_config.get_scene(init_state, name.clone());
            (name.clone(), scene)
        })
        .collect();

    let auto_scene = shared.get_auto_scene(&config.room, config.fixed_brightness);

    let scene_map = {
        let mut scene_map = SceneMap::new(HashMap::new());
        scene_map.merge(shared_scenes.clone());
        scene_map.merge(SceneMap::new(hash_map));
        scene_map.insert(SceneName::new("auto"), auto_scene);
        scene_map
    };

    let lights::Outputs { pc, scene } = run_auto_light(
        inputs,
        &init_state.persistent_state_database,
        scene_map,
        config.flash_color.clone(),
        &config.id,
    );

    scene.send_to_mqtt_string(
        &init_state.mqtt,
        config.id.get_state_topic("scene"),
        &SendOptions::new(),
    );

    send_to_device(&config.id, &config.device, pc, discover, init_state, false);
}

fn split_light(
    init_state: &mut InitState,
    shared_scenes: &SceneMap,
    shared: &SharedAutoLight,
    priority: usize,
    strip_config: &config::StripConfig,
    split_config: &config::SplitLightConfig,
) -> stateful::Receiver<SplitPowerColor> {
    let id = &split_config.id;
    let scenes = &split_config.scenes;
    let flash_color = &split_config.flash_color;

    let inputs = lights::Inputs {
        commands: init_state
            .subscriptions
            .subscribe_into_stateless::<Json<Command>>(id.get_command_topic("")),
    };

    let hash_map: HashMap<SceneName, Scene> = scenes
        .iter()
        .map(|(name, scene_config)| {
            let scene = scene_config.get_scene(init_state, name.clone());
            (name.clone(), scene)
        })
        .collect();

    let auto_scene = shared.get_auto_scene(&strip_config.room, strip_config.fixed_brightness);

    let scene_map = {
        let mut scene_map = SceneMap::new(HashMap::new());
        scene_map.merge(shared_scenes.clone());
        scene_map.merge(SceneMap::new(hash_map));
        scene_map.insert(SceneName::new("auto"), auto_scene);
        scene_map
    };

    let lights::SplitOutputs { spc, scene } = run_split_light(
        inputs,
        &init_state.persistent_state_database,
        scene_map,
        flash_color.clone(),
        id,
        priority,
    );

    scene.send_to_mqtt_string(
        &init_state.mqtt,
        id.get_state_topic("scene"),
        &SendOptions::new(),
    );

    spc
}

fn strip_light(
    init_state: &mut InitState,
    discover: &stateless::Receiver<lifx::Device>,
    shared_scenes: &SceneMap,
    shared: &SharedAutoLight,
    config: &config::StripConfig,
) {
    let span = span!(tracing::Level::INFO, "strip_light", id = %config.id);
    let _guard = span.enter();

    let (combined_tx, combined_rx) = stateful::create_pipe("combined");

    for (priority, split) in config.splits.iter().enumerate() {
        split_light(init_state, shared_scenes, shared, priority, config, split)
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

    let pc = lights::run_merge_light(combined_rx, &config.id, merge_config);

    send_to_device(&config.id, &config.device, pc, discover, init_state, true);
}

#[instrument(skip_all)]
fn send_to_device(
    id: &Id,
    device: &config::LightDeviceConfig,
    pc: stateful::Receiver<PowerColor>,
    discover: &stateless::Receiver<lifx::Device>,
    init_state: &InitState,
    multiple_zones: bool,
) {
    let id_clone = id.clone();
    let output = match device {
        config::LightDeviceConfig::Lifx { lifx_id } => lifx::device_entity(
            pc,
            *lifx_id,
            discover,
            DeviceConfig::default().set_multiple_zones(multiple_zones),
        ),
        config::LightDeviceConfig::Debug { lifx_id } => {
            let lifx_id = *lifx_id;
            pc.map(move |(_, pc)| {
                info!(%id_clone, "Debug {lifx_id}: {pc:?}");
                State::Online(pc)
            })
        }
    };

    output.clone().send_to_mqtt_json(
        &init_state.mqtt,
        id.get_state_topic("status"),
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
            id.get_state_topic("power"),
            &SendOptions::new(),
        );
}
