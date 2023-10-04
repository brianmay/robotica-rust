//! Run tasks based on schedule.
use std::collections::{HashMap, VecDeque};
use std::env::{self, VarError};
use std::fmt::Debug;

use chrono::{Local, NaiveDate, TimeZone, Utc};
use robotica_common::mqtt::{Json, MqttSerializer, QoS};
use thiserror::Error;
use tokio::select;
use tokio::time::Instant;
use tracing::{debug, error, info};

use robotica_common::datetime::{utc_now, Date, DateTime, Duration};
use robotica_common::scheduler::{Importance, Mark, MarkStatus, Status, Tags};

use crate::pipes::{Subscriber, Subscription};
use crate::scheduling::sequencer::check_schedule;
use crate::services::mqtt::{MqttTx, Subscriptions};
use crate::{scheduling::calendar, spawn};

use super::calendar::CalendarEntry;
use super::sequencer::Sequence;
use super::{classifier, scheduler, sequencer};

type CalendarToSequence = dyn Fn(CalendarEntry) -> Option<Sequence> + Send + Sync + 'static;

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
    calendar_to_sequence: Box<CalendarToSequence>,
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
            if let Some(mut sequence) = (*self.calendar_to_sequence)(event) {
                sequence.schedule_date = sequence
                    .start_time
                    .with_timezone(&self.timezone)
                    .date_naive();
                sequence.duration = sequence.end_time - sequence.start_time;
                sequences.push(sequence);
            }
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

        let s = sequencer::schedule_list_to_sequence(
            &self.sequencer,
            date,
            &schedule,
            &c_date,
            &c_tomorrow,
        )
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
    fn get_sequences_all(&self, date: Date) -> Vec<Sequence> {
        // Get Yesterday, Today, and next 3 days.
        let mut sequences: Vec<_> = (-1..=4)
            .flat_map(|day| {
                let date = date + Duration::days(day);
                self.get_sequences_for_date(date)
            })
            .collect();

        sequences.sort_by_key(|s| (s.start_time, s.end_time));
        sequences
    }
}

struct AllMarks(HashMap<String, Mark>);

impl AllMarks {
    fn new() -> Self {
        AllMarks(HashMap::new())
    }

    fn get(&self, sequence: &Sequence) -> Option<Mark> {
        self.0.get(&sequence.id).and_then(|mark| {
            if mark.start_time <= sequence.start_time && sequence.end_time < mark.end_time {
                Some(mark.clone())
            } else {
                None
            }
        })
    }

    fn insert(&mut self, mark: Mark) {
        self.0.insert(mark.id.clone(), mark);
    }

    fn expire(&mut self, now: &DateTime<Utc>) {
        self.0.retain(|_, mark| mark.end_time > *now);
    }
}

struct AllStatus(HashMap<Date, HashMap<(String, usize), Status>>);

impl AllStatus {
    fn new() -> Self {
        AllStatus(HashMap::new())
    }

    fn get(&self, sequence: &Sequence) -> Status {
        let id = (sequence.id.clone(), sequence.repeat_number);
        self.0
            .get(&sequence.schedule_date)
            .and_then(|m| m.get(&id))
            .copied()
            .unwrap_or(Status::Pending)
    }

    fn insert(&mut self, sequence: &Sequence, status: Status) {
        let id = (sequence.id.clone(), sequence.repeat_number);
        let date = sequence.schedule_date;
        self.0.entry(date).or_default().insert(id, status);
    }

    fn expire(&mut self, start: NaiveDate, end: NaiveDate) {
        self.0.retain(|date, _| *date >= start && *date <= end);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EventType {
    Start,
    Stop,
}

#[derive(Debug, Clone)]
struct Event {
    datetime: DateTime<Utc>,
    sequence_index: usize,
    event_type: EventType,
}

struct State<T: TimeZone> {
    date: Date,
    timer: Instant,
    sequences: Vec<Sequence>,
    events: VecDeque<Event>,
    all_marks: AllMarks,
    config: Config<T>,
    mqtt: MqttTx,
    all_status: AllStatus,
    calendar_refresh_time: DateTime<Utc>,
}

impl<T: TimeZone> State<T> {
    fn finalize(&mut self, now: &DateTime<Utc>, publish_sequences: bool) {
        let today = now.with_timezone::<T>(&self.config.timezone).date_naive();

        if today != self.date {
            self.set_tags();
            self.set_sequences_all();
            self.calendar_refresh_time = *now;
            self.publish_all_sequences();
            self.all_marks.expire(now);
        } else if *now > self.calendar_refresh_time + Duration::minutes(5) {
            self.calendar_refresh_time = *now;
            self.set_sequences_all();
            self.publish_all_sequences();
        } else if publish_sequences {
            self.publish_all_sequences();
        }

        self.timer = self.get_next_timer(now);
        self.date = today;
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

    fn publish_all_sequences(&self) {
        self.publish_sequences_pending(&self.sequences);
        self.publish_sequences_important(&self.sequences);
        self.publish_sequences_all(&self.sequences);
    }

    fn publish_sequences_all(&self, sequences: &[Sequence]) {
        let sequences: Vec<Sequence> = sequences
            .iter()
            .cloned()
            .map(|sequence| self.fill_sequence(sequence))
            .collect();

        let topic = format!("schedule/{}/all", self.config.hostname);
        self.publish_sequences(&sequences, topic);
    }

    fn publish_sequences_important(&self, sequences: &[Sequence]) {
        let important: Vec<Sequence> = sequences
            .iter()
            .filter(|sequence| matches!(sequence.importance, Importance::Important))
            .cloned()
            .map(|sequence| self.fill_sequence(sequence))
            .collect();
        let topic = format!("schedule/{}/important", self.config.hostname);
        self.publish_sequences(&important, topic);
    }

    fn publish_sequences_pending(&self, sequences: &[Sequence]) {
        let pending: Vec<Sequence> = sequences
            .iter()
            .cloned()
            .map(|sequence| self.fill_sequence(sequence))
            .filter(|sequence| sequence.status != Some(Status::Completed))
            .collect();
        let topic = format!("schedule/{}/pending", self.config.hostname);
        self.publish_sequences(&pending, topic);
    }

    fn publish_sequences(&self, sequences: &[Sequence], topic: String) {
        let msg = Json(sequences);
        let Ok(message) = msg.serialize(topic, true, QoS::ExactlyOnce) else {
            error!("Failed to serialize sequences: {:?}", sequences);
            return;
        };
        self.mqtt.try_send(message);
    }

    fn get_next_timer(&self, now: &DateTime<Utc>) -> Instant {
        let next = self.events.front();
        next.map_or_else(
            || Instant::now() + tokio::time::Duration::from_secs(120),
            |next| {
                let next = next.datetime;
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
        let tags = self.config.get_tags(today);
        self.publish_tags(&tags);
    }

    fn set_sequences_all(&mut self) {
        let today = self.date;
        self.sequences = self.config.get_sequences_all(today);
        let start = self
            .sequences
            .first()
            .map(|sequence| sequence.schedule_date);

        let end = self.sequences.last().map(|sequence| sequence.schedule_date);
        if let (Some(start), Some(end)) = (start, end) {
            self.all_status.expire(start, end);
        }
        self.set_events();
    }

    fn get_status_for_sequence(&self, sequence: &Sequence) -> Status {
        let status = self.all_status.get(sequence);
        let mark = self.all_marks.get(sequence).map(|m| m.status);

        #[allow(clippy::match_same_arms)]
        match (status, mark) {
            (Status::Completed, _) => Status::Completed,
            (Status::InProgress, _) => Status::InProgress,
            (Status::Cancelled, _) => Status::Cancelled,
            (Status::Pending, Some(MarkStatus::Done)) => Status::Completed,
            (Status::Pending, Some(MarkStatus::Cancelled)) => Status::Cancelled,
            (Status::Pending, None) => Status::Pending,
        }
    }

    fn fill_sequence(&self, sequence: Sequence) -> Sequence {
        let mut sequence = sequence;
        sequence.mark = self.all_marks.get(&sequence);
        sequence.status = Some(self.get_status_for_sequence(&sequence));
        sequence
    }

    fn set_events(&mut self) {
        let mut events = Vec::with_capacity(self.sequences.len() * 2);
        for (index, sequence) in self.sequences.iter().enumerate() {
            let status = self.get_status_for_sequence(sequence);
            // If the sequence is pending, add a start event.
            if matches!(status, Status::Pending) {
                let start = Event {
                    datetime: sequence.start_time,
                    sequence_index: index,
                    event_type: EventType::Start,
                };
                events.push(start);
            }
            // If the sequence is pending or in progress, add a stop event.
            // Note that sequence may be pending now, but should be in progress in time for event
            // (see previous block for adding the start event).
            if matches!(status, Status::Pending | Status::InProgress) {
                let stop = Event {
                    datetime: sequence.end_time,
                    sequence_index: index,
                    event_type: EventType::Stop,
                };
                events.push(stop);
            }
        }
        events.sort_by_key(|event| (event.datetime, event.sequence_index, event.event_type));

        self.events = VecDeque::from(events);
    }

    #[must_use]
    #[allow(clippy::cognitive_complexity)]
    fn process_event(&mut self, event: &Event, now: DateTime<Utc>) -> bool {
        match event.event_type {
            EventType::Start => {
                let sequence = &self.sequences[event.sequence_index];
                let status = self.get_status_for_sequence(sequence);
                if status != Status::Pending {
                    info!(
                        "Skipping starting {sequence:?} because status is {status:?}",
                        sequence = sequence.id,
                        status = status
                    );
                    false
                } else if now > sequence.latest_time {
                    info!(
                        "Skipping starting {sequence:?} because it is too late",
                        sequence = sequence.id
                    );
                    self.all_status.insert(sequence, Status::InProgress);
                    true
                } else {
                    info!("Starting {sequence:?}");
                    for task in &sequence.tasks {
                        for message in task.get_mqtt_messages() {
                            debug!("{now:?}: Sending task {message:?}");
                            self.mqtt.try_send(message.clone());
                        }
                    }
                    self.all_status.insert(sequence, Status::InProgress);
                    true
                }
            }
            EventType::Stop => {
                let sequence = &self.sequences[event.sequence_index];
                let status = self.get_status_for_sequence(sequence);
                if status == Status::InProgress {
                    info!("Stopping {sequence:?}", sequence = sequence.id);
                    self.all_status.insert(sequence, Status::Completed);
                    true
                } else {
                    info!(
                        "Skipping stopping {sequence:?} because status is {status:?}",
                        sequence = sequence.id,
                        status = status
                    );
                    false
                }
            }
        }
    }
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
    calendar_to_sequence: Box<CalendarToSequence>,
) -> Result<(), ExecutorError> {
    let mut state = get_initial_state(mqtt, extra_config, calendar_to_sequence)?;
    let mark_rx = subscriptions.subscribe_into_stateless::<Json<Mark>>("mark");

    spawn(async move {
        let mut mark_s = mark_rx.subscribe().await;

        loop {
            select! {
                _ = tokio::time::sleep_until(state.timer) => {
                    debug!("Timer expired");
                    let mut publish_sequences = false;

                    loop {
                        let now = utc_now();

                        if let Some(next_event) = state.events.front() {
                            if now >= next_event.datetime {
                                // Note: this should never fail. But we need to do this
                                // to take ownership of the event.
                                if let Some(next_event) = state.events.pop_front() {
                                    if state.process_event(&next_event, now) {
                                        publish_sequences = true;
                                    }
                                }
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }


                    let now = utc_now();
                    state.finalize(&now, publish_sequences);

                    {
                    let front = state.events.front();
                    let sequence = front.and_then(|event| state.sequences.get(event.sequence_index));
                    info!("next event is {:?}", front);
                    info!("next sequence is {:?}", sequence.map(|s| &s.id));
                    info!("next timer is {:?}", state.timer - Instant::now());
                    }
                },
                Ok(Json(mark)) = mark_s.recv() => {
                    state.all_marks.insert(mark);
                },
            }
        }
    });

    Ok(())
}

fn get_initial_state(
    mqtt: MqttTx,
    extra_config: ExtraConfig,
    calendar_to_sequence: Box<CalendarToSequence>,
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
                calendar_to_sequence: Box::new(calendar_to_sequence),
            }
        };

        let timer = Instant::now();

        State {
            date,
            timer,
            sequences: Vec::new(),
            events: VecDeque::new(),
            config,
            mqtt,
            all_status: AllStatus::new(),
            all_marks: AllMarks::new(),
            calendar_refresh_time: now,
        }
    };
    let state = {
        let mut state = state;
        state.set_tags();
        state.set_sequences_all();
        // Don't do this here, will happen after first timer.
        // state.publish_sequences(&state.sequences);
        // state.finalize(&now);
        state
    };

    debug!(
        "{:?}: Starting executor, timer at {:?}",
        Utc::now(),
        state.timer
    );
    Ok(state)
}
