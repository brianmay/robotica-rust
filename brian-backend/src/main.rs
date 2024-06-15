//! Main entry point for the application.

#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![allow(clippy::to_string_trait_impl)]

mod amber;
pub(crate) mod audience;
mod config;
mod delays;
mod environment_monitor;
mod ha;
mod hdmi;
mod influxdb;
mod lights;
mod robotica;
mod rooms;
mod tesla;

use std::time::Duration;

use anyhow::Result;
use chrono::{Local, TimeZone};
use delays::rate_limit;
use lights::{run_auto_light, run_passage_light, SharedEntities};
use robotica_backend::devices::lifx::DiscoverConfig;
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
use robotica_common::robotica::lights::LightCommand;
use robotica_common::robotica::message::Message;
use robotica_common::robotica::tasks::{Payload, Task};
use robotica_common::scheduler::Importance;
use robotica_common::zigbee2mqtt::{Door, DoorState};
use robotica_common::{shelly, version};
use tap::Pipe;
use tracing::{debug, error, info};

use crate::amber::hot_water;

use self::tesla::monitor_charging;
use robotica_backend::services::http;
use robotica_backend::services::mqtt::{mqtt_channel, run_client, Subscriptions};
use robotica_backend::services::mqtt::{MqttRx, MqttTx};

#[allow(unreachable_code)]
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    color_backtrace::install();

    info!(
        "Starting brian-backend, version = {:?}, build time = {:?}",
        version::VCS_REF,
        version::BUILD_DATE
    );

    let env = config::Environment::load().unwrap_or_else(|e| {
        panic!("Error loading environment: {e}");
    });

    let config = env.config().unwrap_or_else(|e| {
        panic!("Error loading config: {e}");
    });

    let (mqtt, mqtt_rx) = mqtt_channel();
    let subscriptions: Subscriptions = Subscriptions::new();
    let message_sink = ha::create_message_sink(mqtt.clone());
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

fn calendar_to_sequence(event: CalendarEntry, timezone: Local) -> Option<Sequence> {
    let (start_time, end_time) = calendar_start_top_times(&event, timezone).or_else(|| {
        error!("Error getting start/stop times from calendar event {event:?}");
        None
    })?;

    let payload = Message::new(
        "Calendar Event",
        &event.summary,
        MessagePriority::Low,
        audience::everyone(),
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

    environment_monitor::run(&mut state, &config.influxdb);

    executor(
        &mut state.subscriptions,
        state.mqtt.clone(),
        config.executor,
        Box::new(calendar_to_sequence),
        Local,
    )
    .unwrap_or_else(|err| {
        panic!("Failed to start executor: {err}");
    });

    fake_switch(&mut state, "Dining/Messages");
    fake_switch(&mut state, "Dining/Request_Bathroom");

    fake_switch(&mut state, "Brian/Night");
    fake_switch(&mut state, "Brian/Messages");
    fake_switch(&mut state, "Brian/Request_Bathroom");

    fake_switch(&mut state, "Jan/Messages");

    fake_switch(&mut state, "Twins/Messages");

    fake_switch(&mut state, "Living/Messages");

    fake_switch(&mut state, "Akira/Messages");

    setup_lights(&mut state).await;

    run_client(state.subscriptions, mqtt_rx, config.mqtt).unwrap_or_else(|e| {
        panic!("Error running mqtt client: {e}");
    });
}

fn monitor_bathroom_door(state: &mut InitState) {
    let bathroom_door: stateful::Receiver<DoorState> = state
        .subscriptions
        .subscribe_into_stateful::<Json<Door>>("zigbee2mqtt/Bathroom/door")
        .map(|(_, json)| json.into())
        .pipe(|rx| rate_limit("Bathroom Door Rate Limited", Duration::from_secs(30), rx));

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
                        scene: "busy".to_string(),
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
                    audience::dining_room(),
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

    let _mqtt_clone = state.mqtt.clone();
    let hot_water_request = amber::hot_water::run(state, prices.clone(), is_on);
    let message_sink = state.message_sink.clone();
    hot_water_request.for_each(move |(old, current)| {
        if old.is_none() {
            return;
        }

        let command = match current {
            hot_water::Request::Heat => shelly::SwitchCommand::On(None),
            hot_water::Request::DoNotHeat => shelly::SwitchCommand::Off(None),
        };
        info!("Setting hot water to {:?}", command);
        let _msg = MqttMessage::new(
            "hotwater/command/switch:0",
            command,
            Retain::NoRetain,
            QoS::ExactlyOnce,
        );
        // Comment out for now until confidence reached.
        // mqtt_clone.try_send(msg);

        let message = match current {
            hot_water::Request::Heat => "Turning hot water on",
            hot_water::Request::DoNotHeat => "Turning hot water off",
        };
        message_sink.try_send(Message::new(
            "Hot Water",
            message,
            MessagePriority::DaytimeOnly,
            audience::everyone(),
        ));
    });
}

fn monitor_teslas(
    teslas: &[tesla::Config],
    state: &mut InitState,
    postgres: &sqlx::Pool<sqlx::Postgres>,
    prices: &stateful::Receiver<std::sync::Arc<amber::Prices>>,
) {
    for tesla in teslas {
        let receivers = tesla::Receivers::new(tesla, state);

        let locations = tesla::monitor_teslamate_location(
            state,
            receivers.location.clone(),
            postgres.clone(),
            tesla,
        );

        let charge_request = amber::car::run(
            state,
            tesla.teslamate_id,
            prices.clone(),
            receivers.battery_level.clone(),
            receivers.min_charge_tomorrow.clone(),
            receivers.is_charging.clone(),
        );
        let monitor_charging_receivers = tesla::MonitorChargingInputs::from_receivers(
            &receivers,
            charge_request,
            locations.is_home,
        );
        let outputs =
            monitor_charging(&*state, tesla, monitor_charging_receivers).unwrap_or_else(|e| {
                panic!("Error running tesla charging monitor: {e}");
            });
        let should_plugin_stream = tesla::monitor_tesla_location(
            tesla,
            &*state,
            locations.location,
            outputs.charging_information,
        );

        tesla::commands::run_command_processor(state, tesla, outputs.commands).unwrap_or_else(
            |e| {
                panic!("Error running tesla command processor: {e}");
            },
        );

        let monitor_doors_receivers = tesla::MonitorDoorsReceivers::from_receivers(&receivers);
        tesla::monitor_doors(&*state, tesla, monitor_doors_receivers);
        tesla::plug_in_reminder(&*state, tesla, should_plugin_stream);
    }
}

fn fake_switch(state: &mut InitState, topic_substr: &str) {
    fake_switch::run(&mut state.subscriptions, state.mqtt.clone(), topic_substr);
}

async fn setup_lights(state: &mut InitState) {
    let lifx_config = DiscoverConfig {
        broadcast: "192.168.16.255:56700".to_string(),
        poll_time: std::time::Duration::from_secs(10),
        device_timeout: std::time::Duration::from_secs(45),
        api_timeout: std::time::Duration::from_secs(1),
        num_retries: 3,
    };
    let discover = lifx::discover(lifx_config)
        .await
        .unwrap_or_else(|e| panic!("Error discovering lifx devices: {e}"));

    let shared = SharedEntities::default();
    run_auto_light(
        state,
        discover.clone(),
        shared.clone(),
        "Brian/Light",
        105_867_434_619_856,
    );
    run_auto_light(
        state,
        discover.clone(),
        shared.clone(),
        "Dining/Light",
        74_174_870_942_672,
    );
    run_auto_light(
        state,
        discover.clone(),
        shared.clone(),
        "Jan/Light",
        189_637_382_730_704,
    );
    run_auto_light(
        state,
        discover.clone(),
        shared.clone(),
        "Twins/Light",
        116_355_744_756_688,
    );
    run_auto_light(
        state,
        discover.clone(),
        shared.clone(),
        "Akira/Light",
        280_578_114_286_544,
    );

    run_passage_light(
        state,
        discover,
        shared,
        "Passage/Light",
        137_092_148_851_664,
    );
}
