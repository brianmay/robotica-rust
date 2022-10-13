//! Run tasks based on schedule.
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Debug;

use chrono::{Local, TimeZone, Utc};
use log::{debug, error};
use serde::Serialize;
use thiserror::Error;
use tokio::select;
use tokio::time::Instant;

use crate::scheduling::types::utc_now;
use crate::sources::mqtt::{Message, MqttOut, QoS, Subscriptions};
use crate::spawn;

use super::sequencer::Sequence;
use super::types::{Date, DateTime, Duration, Mark};
use super::{classifier, scheduler, sequencer};

struct Config {
    classifier: Vec<classifier::Config>,
    scheduler: Vec<scheduler::Config>,
    sequencer: sequencer::ConfigMap,
}

#[derive(Error, Debug, Serialize)]
struct Tags {
    yesterday: HashSet<String>,
    today: HashSet<String>,
    tomorrow: HashSet<String>,
}

impl std::fmt::Display for Tags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "yesterday: {:?}, today: {:?}, tomorrow: {:?}",
            self.yesterday, self.today, self.tomorrow
        )
    }
}

struct State<T: TimeZone> {
    date: Date,
    timer: Instant,
    sequences: VecDeque<Sequence>,
    marks: HashMap<String, Mark>,
    timezone: T,
    config: Config,
    mqtt_out: MqttOut,
}

impl<T: TimeZone + Debug> State<T> {
    pub fn finalize(&mut self, now: &DateTime<Utc>) {
        self.check_time_travel(now);
        self.timer = self.get_next_timer(now);
    }

    fn publish_tags(&self, tags: &Tags) {
        let message = serde_json::to_string(&tags).unwrap();
        let message = Message::from_string("test/tags", &message, false, QoS::exactly_once());
        self.mqtt_out.send(message);
    }

    fn publish_sequences(&self, sequences: &VecDeque<Sequence>) {
        let message = serde_json::to_string(&sequences).unwrap();
        let message = Message::from_string("test/sequences", &message, false, QoS::exactly_once());
        self.mqtt_out.send(message);
    }

    fn get_next_timer(&self, now: &DateTime<Utc>) -> Instant {
        let next = self.sequences.front();
        next.map_or_else(
            || Instant::now() + tokio::time::Duration::from_secs(60),
            |next| {
                let next = next.required_time.clone();
                let mut next = next - now.clone();
                // We poll at least every two minutes just in case system time changes.
                if next > Duration::minutes(2) {
                    next = Duration::minutes(2);
                }
                let next = next.to_std().unwrap_or(std::time::Duration::from_secs(0));
                Instant::now() + next
            },
        )
    }

    fn get_sequences_for_date(&self, date: Date) -> Vec<Sequence> {
        let tomorrow = date + Duration::days(1);
        let c_date = classifier::classify_date_with_config(&date, &self.config.classifier);
        let c_tomorrow = classifier::classify_date_with_config(&tomorrow, &self.config.classifier);

        let schedule = scheduler::get_schedule_with_config(
            &date,
            &c_date,
            &c_tomorrow,
            &self.config.scheduler,
            &self.timezone,
        )
        .unwrap_or_else(|e| {
            error!("Error getting schedule for {date}: {e}");
            Vec::new()
        });

        sequencer::schedule_list_to_sequence(
            &self.config.sequencer,
            &schedule,
            &c_date,
            &c_tomorrow,
        )
        .unwrap_or_else(|e| {
            error!("Error getting sequences for {date}: {e}");
            Vec::new()
        })
    }

    fn get_tags(&self, today: Date) -> Tags {
        let yesterday = today - Duration::days(1);
        let tomorrow = today + Duration::days(1);

        Tags {
            yesterday: classifier::classify_date_with_config(&yesterday, &self.config.classifier),
            today: classifier::classify_date_with_config(&today, &self.config.classifier),
            tomorrow: classifier::classify_date_with_config(&tomorrow, &self.config.classifier),
        }
    }

    fn get_entire_schedule(&self, date: Date) -> VecDeque<Sequence> {
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
        sequences.make_contiguous().sort();

        // Set marks.
        set_all_marks(&mut sequences, &self.marks);

        // Return.
        sequences
    }

    fn add_next_day(&mut self, date: Date) -> VecDeque<Sequence> {
        let date = date + Duration::days(1);
        let new_schedule = self.get_sequences_for_date(date);

        // Allocate result vector.
        let mut sequences = VecDeque::with_capacity(self.sequences.len() + new_schedule.len());

        // Add existing schedule.
        sequences.extend(self.sequences.clone().into_iter());

        // Add schedule for tomorrow.
        sequences.extend(new_schedule.into_iter());

        // Sort by time.
        sequences.make_contiguous().sort();

        // Set marks.
        set_all_marks(&mut sequences, &self.marks);

        // return.
        sequences
    }

    fn check_time_travel(&mut self, now: &DateTime<Utc>) {
        let today = now.with_timezone::<T>(&self.timezone).date();
        let yesterday = today - Duration::days(1);

        #[allow(clippy::if_same_then_else)]
        if today < self.date {
            // If we have travelled back in time, we should drop the list entirely
            // to avoid duplicating future events.
            let tags = self.get_tags(today);
            self.publish_tags(&tags);
            self.sequences = self.get_entire_schedule(today);
        } else if yesterday == self.date {
            // If we have travelled forward in time by one day, we only need to
            // add events for tomorrow.
            // let mut steps = self.sequence.chain(self.get_steps_for_date(&tomorrow));
            // self.sequence = Box::new(steps);
            let tags = self.get_tags(today);
            self.publish_tags(&tags);
            self.sequences = self.add_next_day(today);
        } else if today > self.date {
            // If we have travelled forward in time more then one day, regenerate
            // entire events list.
            let tags = self.get_tags(today);
            self.publish_tags(&tags);
            self.sequences = self.get_entire_schedule(today);
        } else {
            // No change in date.
        };

        self.date = today;
        self.publish_sequences(&self.sequences);
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

fn expire_marks(marks: &mut HashMap<String, Mark>, now: &DateTime<Utc>) {
    marks.retain(|_, mark| mark.stop_time > *now);
}

/// An error occurred in the executor.
#[derive(Error, Debug)]
pub enum ExecutorError {
    /// A classifier config error occurred.
    #[error("Classifier Config Error {0}")]
    ClassifierConfigError(#[from] classifier::ConfigError),

    /// A Scheduler config error occurred.
    #[error("Scheduler Config Error {0}")]
    SchedulerConfigError(#[from] scheduler::ConfigError),

    /// A Sequencer config error occurred.
    #[error("Sequencer Config Error {0}")]
    SequencerConfigError(#[from] sequencer::ConfigError),
}

/// Create a timer that sends outgoing messages at regularly spaced intervals.
///
/// # Errors
///
/// This function will return an error if the `config` is invalid.
pub fn executor(subscriptions: &mut Subscriptions, mqtt_out: MqttOut) -> Result<(), ExecutorError> {
    let mut state = get_initial_state(mqtt_out)?;
    let mark_rx = subscriptions.subscribe_into::<Mark>("mark");

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

                        if now < sequence.required_time {
                            // Too early, wait for next timer.
                            debug!("Too early for {sequence:?}");
                            break;
                        } else if sequence.mark.is_some() {
                            debug!("Ignoring step with mark {:?}", sequence.mark);
                            state.sequences.pop_front();
                        } else if now < sequence.latest_time {
                            // Send message.
                            debug!("Processing step {sequence:?}");
                            for task in &sequence.tasks {
                                for message in task.get_messages() {
                                    debug!("{now:?}: Sending task {message:?}");
                                    state.mqtt_out.send(message.clone());
                                }
                            }
                            state.sequences.pop_front();
                        } else {
                            // Too late, drop event.
                            debug!("Too late for {sequence:?}");
                            state.sequences.pop_front();
                        }
                    }

                    let now = utc_now();
                    state.finalize(&now);
                    expire_marks(&mut state.marks, &now);
                },
                Ok((_, mark)) = mark_s.recv() => {
                    state.marks.insert(mark.id.clone(), mark);
                    debug!("Marks: {:?}", state.marks);
                    set_all_marks(&mut state.sequences, &state.marks);
                },
            }
        }
    });

    Ok(())
}

fn get_initial_state(mqtt_out: MqttOut) -> Result<State<Local>, ExecutorError> {
    let timezone = Local;
    let now = DateTime::from(Utc::now());
    let date = now.with_timezone::<Local>(&timezone).date();

    let state = {
        let config = {
            let classifier = classifier::load_config_from_default_file()?;
            let scheduler = scheduler::load_config_from_default_file()?;
            let sequencer = sequencer::load_config_from_default_file()?;
            Config {
                classifier,
                scheduler,
                sequencer,
            }
        };

        let timer = Instant::now();
        let sequence = VecDeque::new();
        let marks = HashMap::new();

        State {
            date,
            timer,
            sequences: sequence,
            marks,
            timezone,
            config,
            mqtt_out,
        }
    };
    let sequences = state.get_entire_schedule(state.date);
    let mut state = State { sequences, ..state };

    {
        let tags = state.get_tags(state.date);
        state.publish_tags(&tags);
    }

    state.finalize(&now);

    debug!(
        "{:?}: Starting executor, Next task {:?}, timer at {:?}",
        Utc::now(),
        state.sequences.front(),
        state.timer
    );
    Ok(state)
}
