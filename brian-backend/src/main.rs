//! Main entry point for the application.

#![warn(missing_docs)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod amber;
pub(crate) mod audience;
mod config;
mod delays;
mod environment_monitor;
mod ha;
mod hdmi;
mod lights;
mod robotica;
mod rooms;
mod tesla;

use anyhow::Result;
use chrono::{Duration, Local, TimeZone};
use lights::{run_auto_light, run_passage_light, SharedEntities};
use robotica_backend::devices::lifx::DiscoverConfig;
use robotica_backend::devices::{fake_switch, lifx};
use robotica_backend::pipes::stateless;
use robotica_backend::scheduling::calendar::{CalendarEntry, StartEnd};
use robotica_backend::scheduling::executor::executor;
use robotica_backend::scheduling::sequencer::Sequence;
use robotica_backend::services::persistent_state::PersistentStateDatabase;
use robotica_common::mqtt::QoS;
use robotica_common::robotica::audio::MessagePriority;
use robotica_common::robotica::commands::Command;
use robotica_common::robotica::message::Message;
use robotica_common::robotica::tasks::{Payload, Task};
use robotica_common::scheduler::Importance;
use tracing::{debug, error, info};

use self::tesla::monitor_charging;
use robotica_backend::services::http;
use robotica_backend::services::mqtt::MqttTx;
use robotica_backend::services::mqtt::{mqtt_channel, run_client, Subscriptions};

#[allow(unreachable_code)]
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    color_backtrace::install();

    let (mqtt, mqtt_rx) = mqtt_channel();
    let subscriptions: Subscriptions = Subscriptions::new();
    let message_sink = ha::create_message_sink(mqtt.clone());
    let persistent_state_database = PersistentStateDatabase::new().unwrap_or_else(|e| {
        panic!("Error getting persistent state loader: {e}");
    });

    let mut state = State {
        subscriptions,
        mqtt,
        message_sink,
        persistent_state_database,
    };

    setup_pipes(&mut state).await;
    run_client(state.subscriptions, mqtt_rx)?;

    loop {
        debug!("I haven't crashed yet!");
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
    }

    Ok(())
}

/// Global state for the application.
pub struct State {
    subscriptions: Subscriptions,
    #[allow(dead_code)]
    mqtt: MqttTx,
    message_sink: stateless::Sender<Message>,
    persistent_state_database: PersistentStateDatabase,
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
            retain: false,
            topics: ["ha/event/message".to_string()].to_vec(),
        }],
    };

    #[allow(deprecated)]
    Some(Sequence {
        title: event.summary.clone(),
        id: event.uid,
        importance: Importance::Important,
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
        duration: Duration::zero(),
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

async fn setup_pipes(state: &mut State) {
    let config = config::load_config_from_default_file().unwrap_or_else(|e| {
        panic!("Error loading config: {e}");
    });

    let price_summary_rx = amber::run(state).unwrap_or_else(|e| {
        panic!("Error running amber: {e}");
    });

    price_summary_rx
        .clone()
        .map(|(_, current)| current.is_cheap_2hr)
        .for_each(move |(old, current)| {
            if old.is_some() {
                let message = if current {
                    "2 hour cheap price has started"
                } else {
                    "2 hour cheap price has ended"
                };
                info!("{}", message);
            }
        });

    monitor_charging(state, 1, price_summary_rx).unwrap_or_else(|e| {
        panic!("Error running tesla charging monitor: {e}");
    });

    let rooms = rooms::get();
    http::run(state.mqtt.clone(), rooms)
        .await
        .unwrap_or_else(|e| panic!("Error running http server: {e}"));

    hdmi::run(state, "Dining", "TV", "hdmi.pri:8000");
    tesla::monitor_tesla_location(state, 1);
    tesla::monitor_tesla_doors(state, 1);

    environment_monitor::run(state).unwrap_or_else(|err| {
        panic!("Environment monitor failed: {err}");
    });

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

    fake_switch(state, "Dining/Messages");
    fake_switch(state, "Dining/Request_Bathroom");

    fake_switch(state, "Brian/Night");
    fake_switch(state, "Brian/Messages");
    fake_switch(state, "Brian/Request_Bathroom");

    fake_switch(state, "Jan/Messages");

    fake_switch(state, "Twins/Messages");

    fake_switch(state, "Extension/Messages");

    fake_switch(state, "Akira/Messages");

    setup_lights(state).await;
}

fn fake_switch(state: &mut State, topic_substr: &str) {
    fake_switch::run(&mut state.subscriptions, state.mqtt.clone(), topic_substr);
}

async fn setup_lights(state: &mut State) {
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
