//! Run tasks based on schedule.
use std::collections::{HashMap, HashSet, VecDeque};
use std::env::{self, VarError};
use std::fmt::Debug;

use chrono::{Local, TimeZone, Utc};
use robotica_common::mqtt::{Json, MqttSerializer, QoS};
use robotica_common::robotica::tasks::{Payload, Task};
use serde::Serialize;
use thiserror::Error;
use tokio::select;
use tokio::time::Instant;
use tracing::{debug, error, info};

use robotica_common::datetime::{utc_now, Date, DateTime, Duration};
use robotica_common::scheduler::{Importance, Mark};

use crate::pipes::{Subscriber, Subscription};
use crate::scheduling::sequencer::check_schedule;
use crate::services::mqtt::{MqttTx, Subscriptions};
use crate::tasks::get_task_messages;
use crate::{scheduling::calendar, spawn};

use super::sequencer::Sequence;
use super::{classifier, scheduler, sequencer};

/// Extra configuration settings for the executor.
#[derive(serde::Deserialize)]
pub struct ExtraConfig {
    /// The URL of the calendar to use for extra events.
    pub calendar_url: String,
}

struct Config<T: TimeZone> {
    classifier: Vec<classifier::Config>,
    scheduler: Vec<scheduler::Config>,
    sequencer: sequencer::ConfigMap,
    hostname: String,
    extra_config: ExtraConfig,
    timezone: T,
}
impl<T: TimeZone> Config<T> {
    fn load_calendar(&self, start: Date, stop: Date) -> Vec<Sequence> {
        let calendar =
            calendar::load(&self.extra_config.calendar_url, start, stop).unwrap_or_else(|e| {
                error!("Error loading calendar: {e}");
                Vec::new()
            });

        let mut sequences = Vec::new();

        for event in calendar {
            let (start, stop) = match event.start_end {
                calendar::StartEnd::Date(_, _) => continue,
                calendar::StartEnd::DateTime(start, stop) => (start, stop),
            };

            let duration = stop - start;

            // FIXME: This should not be hardcoded here.
            let payload = serde_json::json!( {
                "type": "message",
                "title": "Calendar Event",
                "body": event.summary.clone(),
                "priority": "Low",
            });

            let task = Task {
                title: event.summary.clone(),
                payload: Payload::Json(payload),
                qos: QoS::ExactlyOnce,
                retain: false,
                topics: ["ha/event/message/everyone".to_string()].to_vec(),
            };

            let sequence = Sequence {
                id: event.uid,
                importance: Importance::Important,
                sequence_name: event.summary,
                required_time: start,
                latest_time: stop,
                required_duration: duration,
                tasks: vec![task],
                mark: None,
                if_cond: None,
                classifications: None,
                options: None,
                zero_time: true,
                repeat_number: 1,
            };

            sequences.push(sequence);
        }

        sequences
    }

    fn get_sequences_for_date(&self, date: Date) -> Vec<Sequence> {
        let tomorrow = date + Duration::days(1);
        let c_date = classifier::classify_date_with_config(&date, &self.classifier);
        let c_tomorrow = classifier::classify_date_with_config(&tomorrow, &self.classifier);

        let schedule = scheduler::get_schedule_with_config(
            &date,
            &c_date,
            &c_tomorrow,
            &self.scheduler,
            &self.timezone,
        )
        .unwrap_or_else(|e| {
            error!("Error getting schedule for {date}: {e}");
            Vec::new()
        });

        let s =
            sequencer::schedule_list_to_sequence(&self.sequencer, &schedule, &c_date, &c_tomorrow)
                .unwrap_or_else(|e| {
                    error!("Error getting sequences for {date}: {e}");
                    Vec::new()
                });

        let calendar = self.load_calendar(date, date + Duration::days(1));
        let mut sequences = Vec::with_capacity(s.len() + calendar.len());
        sequences.extend(s);
        sequences.extend(calendar);
        sequences
    }

    fn get_tags(&self, today: Date) -> Tags {
        let yesterday = today - Duration::days(1);
        let tomorrow = today + Duration::days(1);

        Tags {
            yesterday: classifier::classify_date_with_config(&yesterday, &self.classifier),
            today: classifier::classify_date_with_config(&today, &self.classifier),
            tomorrow: classifier::classify_date_with_config(&tomorrow, &self.classifier),
        }
    }
    fn get_sequences_all(&self, date: Date) -> VecDeque<Sequence> {
        let date = date - Duration::days(1);
        let yesterday = self.get_sequences_for_date(date);

        let date = date + Duration::days(1);
        let today = self.get_sequences_for_date(date);

        let date = date + Duration::days(1);
        let tomorrow = self.get_sequences_for_date(date);

        // Allocate result vector.
        let mut sequences = VecDeque::with_capacity(yesterday.len() + today.len() + tomorrow.len());

        // Add schedule for yesterday, today, and tomorrow.
        sequences.extend(yesterday);
        sequences.extend(today);
        sequences.extend(tomorrow);

        // Sort by time.
        sequences
            .make_contiguous()
            .sort_by(Sequence::cmp_required_time);

        // Return.
        sequences
    }

    fn add_sequences_tomorrow(&self, date: Date, sequences: &mut VecDeque<Sequence>) {
        let tomorrow = date + Duration::days(1);
        let new_schedule = self.get_sequences_for_date(tomorrow);

        // Add schedule for tomorrow.
        sequences.extend(new_schedule);

        // Sort by time.
        sequences
            .make_contiguous()
            .sort_by(Sequence::cmp_required_time);
    }
}

fn set_all_marks(sequences: &mut VecDeque<Sequence>, marks: &HashMap<String, Mark>) {
    for sequence in &mut *sequences {
        let mark = if let Some(mark) = marks.get(&sequence.id) {
            if sequence.required_time >= mark.start_time && sequence.required_time < mark.stop_time
            {
                Some(mark.clone())
            } else {
                None
            }
        } else {
            None
        };

        sequence.mark = mark;
    }
}

#[derive(Debug, Serialize)]
struct Tags {
    yesterday: HashSet<String>,
    today: HashSet<String>,
    tomorrow: HashSet<String>,
}

struct State<T: TimeZone> {
    date: Date,
    timer: Instant,
    sequences: VecDeque<Sequence>,
    tags: Tags,
    marks: HashMap<String, Mark>,
    config: Config<T>,
    mqtt: MqttTx,
    done: HashSet<(Date, String)>,
    calendar_refresh_time: DateTime<Utc>,
}

impl<T: TimeZone> State<T> {
    fn finalize(&mut self, now: &DateTime<Utc>) {
        let today = now.with_timezone::<T>(&self.config.timezone).date_naive();

        if self.check_time_travel(today) {
            self.calendar_refresh_time = *now;
            self.publish_tags(&self.tags);
        } else if *now > self.calendar_refresh_time + Duration::minutes(5) {
            self.calendar_refresh_time = *now;
            self.set_sequences_all();
        }

        self.publish_sequences(&self.sequences);
        self.timer = self.get_next_timer(now);
        self.date = today;
    }

    fn check_time_travel(&mut self, today: Date) -> bool {
        let yesterday = today - Duration::days(1);

        if today < self.date {
            // If we have travelled back in time, we should drop the list entirely
            // to avoid duplicating future events.
            self.set_tags();
            self.set_sequences_all();
            self.done = HashSet::new();
            true
        } else if yesterday == self.date {
            // If we have travelled forward in time by one day, we only need to
            // add events for tomorrow.
            self.set_tags();
            self.add_sequences_tomorrow();
            self.done = HashSet::new();
            true
        } else if today > self.date {
            // If we have travelled forward in time more then one day, regenerate
            // entire events list.
            self.set_tags();
            self.set_sequences_all();
            self.done = HashSet::new();
            true
        } else {
            // No change in date.
            false
        }
    }

    fn publish_tags(&self, tags: &Tags) {
        info!("Tags: {:?}", tags);
        let topic = format!("robotica/{}/tags", self.config.hostname);
        let msg = Json(tags);
        let Ok(message) = msg.serialize(topic, true, QoS::ExactlyOnce) else {
            error!("Failed to serialize tags: {:?}", tags);
            return;
        };
        self.mqtt.try_send(message);
    }

    fn publish_sequences(&self, sequences: &VecDeque<Sequence>) {
        let topic = format!("schedule/{}", self.config.hostname);
        let msg = Json(sequences);
        let Ok(message) = msg.serialize(topic, true, QoS::ExactlyOnce) else {
            error!("Failed to serialize sequences: {:?}", sequences);
            return;
        };
        self.mqtt.try_send(message);
    }

    fn get_next_timer(&self, now: &DateTime<Utc>) -> Instant {
        let next = self.sequences.front();
        next.map_or_else(
            || Instant::now() + tokio::time::Duration::from_secs(120),
            |next| {
                let next = next.required_time;
                let mut next = next - *now;
                // We poll at least every two minutes just in case system time changes.
                if next > chrono::Duration::minutes(1) {
                    next = chrono::Duration::minutes(1);
                }
                let next = next.to_std().unwrap_or(std::time::Duration::from_secs(60));
                Instant::now() + next
            },
        )
    }

    fn set_tags(&mut self) {
        let today = self.date;
        self.tags = self.config.get_tags(today);
    }

    fn set_sequences_all(&mut self) {
        let today = self.date;
        self.sequences = self.config.get_sequences_all(today);
        self.drop_done_sequences();
        set_all_marks(&mut self.sequences, &self.marks);
    }

    fn add_sequences_tomorrow(&mut self) {
        let today = self.date;
        self.config
            .add_sequences_tomorrow(today, &mut self.sequences);
        set_all_marks(&mut self.sequences, &self.marks);
    }

    fn drop_done_sequences(&mut self) {
        self.sequences.retain(|sequence| {
            let sequence_date = sequence.required_time.date_naive();
            !self.done.contains(&(sequence_date, sequence.id.clone()))
        });
    }
}

fn expire_marks(marks: &mut HashMap<String, Mark>, now: &DateTime<Utc>) {
    marks.retain(|_, mark| mark.stop_time > *now);
}

/// An error occurred in the executor.
#[derive(Error, Debug)]
pub enum ExecutorError {
    /// A classifier config error occurred.
    #[error("Classifier Config Error: {0}")]
    ClassifierConfigError(#[from] classifier::ConfigError),

    /// A Scheduler config error occurred.
    #[error("Scheduler Config Error: {0}")]
    SchedulerConfigError(#[from] scheduler::ConfigError),

    /// A Sequencer config error occurred.
    #[error("Sequencer Config Error: {0}")]
    SequencerConfigError(#[from] sequencer::ConfigError),

    /// A Scheduler config error occurred.
    #[error("Sequencer Config Check Error: {0}")]
    SequencerConfigCheckError(#[from] sequencer::ConfigCheckError),

    /// The hostname could not be determined.
    #[error("Could not determine hostname: {0}")]
    HostnameError(#[from] VarError),
}

/// Create a timer that sends outgoing messages at regularly spaced intervals.
///
/// # Errors
///
/// This function will return an error if the `config` is invalid.
pub fn executor(
    subscriptions: &mut Subscriptions,
    mqtt: MqttTx,
    extra_config: ExtraConfig,
) -> Result<(), ExecutorError> {
    let mut state = get_initial_state(mqtt, extra_config)?;
    let mark_rx = subscriptions.subscribe_into_stateless::<Json<Mark>>("mark");

    spawn(async move {
        let mut mark_s = mark_rx.subscribe().await;

        loop {
            debug!(
                "Next task {:?}, timer at {:?}",
                state.sequences.front(),
                state.timer
            );

            select! {
                _ = tokio::time::sleep_until(state.timer) => {
                    debug!("Timer expired");

                    while let Some(sequence) = state.sequences.front() {
                        let now = utc_now();
                        let sequence_date = sequence.required_time.date_naive();

                        if state.done.contains(&(sequence_date, sequence.id.clone())) {
                            debug!("Already done with {sequence:?}");
                        } else if now < sequence.required_time {
                            // Too early, wait for next timer.
                            debug!("Too early for {sequence:?}");
                            break;
                        } else if sequence.mark.is_some() {
                            debug!("Ignoring step with mark {:?}", sequence.mark);
                        } else if now < sequence.latest_time {
                            // Send message.
                            info!("Processing step {sequence:?}");
                            for task in &sequence.tasks {
                                for message in get_task_messages(task) {
                                    debug!("{now:?}: Sending task {message:?}");
                                    state.mqtt.try_send(message.clone());
                                }
                            }
                        } else {
                            // Too late, drop event.
                            debug!("Too late for {sequence:?}");
                        }
                        state.done.insert((sequence_date, sequence.id.clone()));
                        state.sequences.pop_front();
                    }

                    let now = utc_now();
                    state.finalize(&now);
                    expire_marks(&mut state.marks, &now);
                },
                Ok(Json(mark)) = mark_s.recv() => {
                    state.marks.insert(mark.id.clone(), mark);
                    debug!("Marks: {:?}", state.marks);
                    set_all_marks(&mut state.sequences, &state.marks);
                },
            }
        }
    });

    Ok(())
}

fn get_initial_state(
    mqtt: MqttTx,
    extra_config: ExtraConfig,
) -> Result<State<Local>, ExecutorError> {
    let timezone = Local;
    let now = Utc::now();
    let date = now.with_timezone::<Local>(&timezone).date_naive();
    let hostname = env::var("HOSTNAME")?;

    let state = {
        let config = {
            let classifier = classifier::load_config_from_default_file()?;
            let scheduler = scheduler::load_config_from_default_file()?;
            let sequencer = sequencer::load_config_from_default_file()?;
            check_schedule(&scheduler, &sequencer)?;
            Config {
                classifier,
                scheduler,
                sequencer,
                hostname,
                extra_config,
                timezone,
            }
        };

        let timer = Instant::now();
        let marks = HashMap::new();

        State {
            date,
            timer,
            sequences: VecDeque::new(),
            marks,
            config,
            mqtt,
            done: HashSet::new(),
            calendar_refresh_time: now,
            tags: Tags {
                yesterday: HashSet::new(),
                today: HashSet::new(),
                tomorrow: HashSet::new(),
            },
        }
    };
    let state = {
        let mut state = state;
        state.tags = state.config.get_tags(state.date);
        state.publish_tags(&state.tags);
        state.sequences = state.config.get_sequences_all(state.date);
        state.drop_done_sequences();
        set_all_marks(&mut state.sequences, &state.marks);
        // Don't do this here, will happen after first timer.
        // state.publish_sequences(&state.sequences);
        // state.finalize(&now);
        state
    };

    debug!(
        "{:?}: Starting executor, Next task {:?}, timer at {:?}",
        Utc::now(),
        state.sequences.front(),
        state.timer
    );
    Ok(state)
}
