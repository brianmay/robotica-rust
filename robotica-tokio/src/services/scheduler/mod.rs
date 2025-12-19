//! Scheduler service for sending scheduled values at specific times of day.
//!
//! Uses local time for scheduling, but converts to UTC for actual scheduling to handle DST changes.
use chrono::{NaiveTime, TimeZone};
use robotica_common::datetime::{duration, utc_now};
use tracing::debug;

use crate::{pipes::stateful, spawn};

/// An entry in the scheduler.
#[derive(Debug)]
pub struct Entry<T> {
    /// The time of day to send the value.
    pub scheduled_time: NaiveTime,
    /// The value to send.
    pub value: T,
}
struct UtcEntry<T> {
    scheduled_time: chrono::DateTime<chrono::Utc>,
    latest_time: chrono::DateTime<chrono::Utc>,
    value: T,
}

const MIDNIGHT: NaiveTime = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
const FALLBACK: chrono::NaiveTime = NaiveTime::from_hms_opt(3, 0, 0).unwrap();

/// Create a scheduler pipe that sends the scheduled values at the specified times of day.
#[must_use]
pub fn scheduler<T>(name: &str, entries: Vec<Entry<T>>) -> stateful::Receiver<T>
where
    T: std::fmt::Debug + Clone + PartialEq + Send + Sync + 'static,
{
    let (tx_out, rx_out) = stateful::create_pipe(name);
    let name = name.to_string();

    spawn(async move {
        let now = utc_now();
        let mut got_date = now.with_timezone(&chrono::Local).date_naive();
        let mut utc_entries = Vec::with_capacity(entries.len() * 2);

        {
            let yesterday = utc_now().date_naive().pred_opt();
            if let Some(yesterday) = yesterday {
                utc_entries.extend(get_utc_entries_for_date(&entries, yesterday));
            }

            utc_entries.extend(get_utc_entries_for_date(&entries, got_date));
            utc_entries.sort_by_key(|e| e.scheduled_time);
        }

        let latest_time = utc_entries
            .iter()
            .filter(|e| e.scheduled_time < now)
            .max_by_key(|e| e.scheduled_time);

        if let Some(latest_time) = latest_time {
            // If we found one, send it as the initial value
            debug!(
                "{name}: Sending initial scheduled entry at {}: {:?}",
                latest_time.scheduled_time, latest_time.value
            );
            tx_out.try_send(latest_time.value.clone());
        }

        loop {
            let now = utc_now();
            let date = now.with_timezone(&chrono::Local).date_naive();

            if date != got_date {
                debug!("{name}: Date changed, recalculating schedule.");
                utc_entries.clear();
                utc_entries.extend(get_utc_entries_for_date(&entries, date));
                utc_entries.sort_by_key(|e| e.scheduled_time);
                got_date = date;
            }

            // Get and remove the first entry in utc_entries
            let next_entry = loop {
                match utc_entries.first() {
                    // If the entry is in the past, skip it.
                    Some(e) if e.latest_time < now => {
                        // This should never fail, but just in case
                        if let Some(e) = safe_remove_first(&mut utc_entries) {
                            debug!(
                                "{name}: Skipping past scheduled entry at {}: {:?}",
                                e.scheduled_time, e.value
                            );
                        }
                    }
                    // If the entry is currently due, send it.
                    Some(e) if e.scheduled_time <= now => {
                        // This should never fail, but just in case
                        if let Some(e) = safe_remove_first(&mut utc_entries) {
                            debug!(
                                "{name}: Sending scheduled entry at {}: {:?}",
                                e.scheduled_time, e.value
                            );
                            tx_out.try_send(e.value.clone());
                        }
                    }
                    // If the entry is in the future, return it.
                    // It is important that we do not remove it from the list yet.
                    Some(e) => break Some(e),
                    // No more entries
                    None => break None,
                }
            };

            // Determine when to sleep until
            let sleep_until = next_entry.map_or_else(
                || {
                    let midnight = get_next_midnight_from_date(now, date);
                    debug!("{name}: No more scheduled entries today, sleeping until {midnight}.");
                    midnight
                },
                |entry| {
                    debug!(
                        "Next scheduled entry at {}, sleeping until then.",
                        entry.scheduled_time
                    );
                    entry.scheduled_time
                },
            );

            // Calculate duration of sleep
            let duration = sleep_until - now;

            // Clamp to max 5 minutes to avoid sleeping too long in case of clock changes
            let duration = duration.clamp(chrono::Duration::zero(), chrono::Duration::minutes(5));

            // Debug log
            debug!("{name}: Sleeping for {} seconds.", duration.num_seconds());

            // Convert to tokio duration
            let sleep_duration = duration.to_std().unwrap_or_default();

            tokio::time::sleep(sleep_duration).await;
        }
    });

    rx_out
}

fn safe_remove_first<T>(v: &mut Vec<T>) -> Option<T> {
    if v.is_empty() {
        None
    } else {
        Some(v.remove(0))
    }
}

fn get_utc_entries_for_date<T: Clone>(
    entries: &[Entry<T>],
    date: chrono::NaiveDate,
) -> impl Iterator<Item = UtcEntry<T>> + '_ {
    let utc_entries = entries.iter().map(move |e| {
        let scheduled_time = chrono::Local
            .from_local_datetime(&chrono::NaiveDateTime::new(date, e.scheduled_time))
            .earliest()
            .or_else(|| {
                debug!("Could not convert local datetime to UTC, using 3am as fallback.");
                chrono::Local
                    .from_local_datetime(&chrono::NaiveDateTime::new(date, FALLBACK))
                    .earliest()
            })
            .map_or_else(
                || {
                    debug!("Could not convert 3am to UTC, using now as fallback.");
                    utc_now()
                },
                |dt| dt.with_timezone(&chrono::Utc),
            );
        UtcEntry {
            scheduled_time,
            latest_time: scheduled_time + duration::minutes(1),
            value: e.value.clone(),
        }
    });
    utc_entries
}

fn get_next_midnight_from_date(
    now: chrono::DateTime<chrono::Utc>,
    date: chrono::NaiveDate,
) -> chrono::DateTime<chrono::Utc> {
    date.succ_opt().map_or_else(
        || {
            debug!("Could not get next date, defaulting to now.");
            now
        },
        |next_date| {
            let local = chrono::NaiveDateTime::new(next_date, MIDNIGHT);
            chrono::Local
                .from_local_datetime(&local)
                .earliest()
                .map_or(now, |dt| dt.with_timezone(&chrono::Utc))
        },
    )
}
