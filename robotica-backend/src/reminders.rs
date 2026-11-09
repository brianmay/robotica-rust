//! In-memory reminder service.
//!
//! Reminders are created by code holding a [`Reminder`] handle. The service
//! sends active reminders to the message sink repeatedly until they are
//! acknowledged or the handle is dropped. The message can be updated in
//! place while the reminder exists. The published reminder list is retained
//! on MQTT so the frontend always has the current state.
//!
//! Silences are stored in the database and survive restarts; reminders do
//! not. The service publishes an empty list at startup, which clears both
//! the in-memory state and any stale retained MQTT list.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use robotica_common::datetime::utc_now;
use robotica_common::robotica::message::Message;
use robotica_common::robotica::reminder::{
    ReminderCommand, ReminderId, ReminderInfo, ReminderStatus,
};
use robotica_tokio::database::reminders::{
    delete_silences_for_key, insert_silence, load_active_silences, Silence,
};
use robotica_tokio::pipes::{stateful, stateless};
use robotica_tokio::spawn;
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Configuration for the reminder service.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// How often active reminders are sent, in minutes.
    ///
    /// Values below one are treated as one minute.
    #[serde(default = "default_repeat_interval_minutes")]
    pub repeat_interval_minutes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            repeat_interval_minutes: default_repeat_interval_minutes(),
        }
    }
}

const fn default_repeat_interval_minutes() -> u64 {
    5
}

impl Config {
    fn repeat_interval(&self) -> Duration {
        Duration::from_mins(self.repeat_interval_minutes.max(1))
    }
}

/// How long an applicable silence lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SilenceExpiry {
    /// The silence never expires.
    Indefinite,

    /// The silence expires at the given time.
    Until(DateTime<Utc>),
}

impl SilenceExpiry {
    const fn until(self) -> Option<DateTime<Utc>> {
        match self {
            Self::Indefinite => None,
            Self::Until(until) => Some(until),
        }
    }
}

struct Entry {
    id: ReminderId,
    key: String,
    message: Message,
    created_at: DateTime<Utc>,
    acknowledged: bool,
    silenced: Option<SilenceExpiry>,
}

impl Entry {
    const fn status(&self) -> ReminderStatus {
        if self.acknowledged {
            ReminderStatus::Acknowledged
        } else if self.silenced.is_some() {
            ReminderStatus::Silenced
        } else {
            ReminderStatus::Active
        }
    }

    /// Replace the message, resetting acknowledgement.
    ///
    /// Returns `true` if the message changed; an identical message leaves
    /// the entry untouched.
    fn update_message(&mut self, message: Message) -> bool {
        if self.message == message {
            return false;
        }

        self.message = message;
        self.acknowledged = false;
        true
    }

    fn info(&self) -> ReminderInfo {
        ReminderInfo {
            id: self.id.clone(),
            key: self.key.clone(),
            message: self.message.clone(),
            created_at: self.created_at,
            status: self.status(),
            silenced_until: self.silenced.and_then(SilenceExpiry::until),
        }
    }
}

enum Command {
    Create {
        id: ReminderId,
        key: String,
        message: Message,
    },
    Update {
        id: ReminderId,
        message: Message,
    },
    Delete(ReminderId),
    Acknowledge(ReminderId),
    Silence {
        key: String,
        duration_minutes: Option<u32>,
    },
    Unsilence {
        key: String,
    },
}

/// A handle to a single reminder.
///
/// Dropping the handle deletes the reminder from the service. The message
/// can be changed with [`Reminder::update_message`].
pub struct Reminder {
    id: ReminderId,
    tx: mpsc::UnboundedSender<Command>,
}

impl Reminder {
    /// Update the message of this reminder.
    ///
    /// The reminder is updated in place: the id, key, creation time and
    /// silences are all preserved. If the message actually changed, the
    /// reminder is sent immediately when it is active, and a previously
    /// acknowledged reminder becomes active again. A silenced reminder is
    /// not sent.
    pub fn update_message(&self, message: Message) {
        if self
            .tx
            .send(Command::Update {
                id: self.id.clone(),
                message,
            })
            .is_err()
        {
            debug!(
                reminder_id = %self.id,
                "Reminder service is no longer running; ignoring message update"
            );
        }
    }
}

impl Drop for Reminder {
    fn drop(&mut self) {
        if self.tx.send(Command::Delete(self.id.clone())).is_err() {
            debug!(
                reminder_id = %self.id,
                "Reminder service is no longer running; dropping reminder locally"
            );
        }
    }
}

/// A handle to the reminder service.
#[derive(Clone)]
pub struct ReminderService {
    tx: mpsc::UnboundedSender<Command>,
}

impl ReminderService {
    /// Start the reminder service.
    ///
    /// Returns the service handle and a stateful receiver containing the
    /// current reminder list, which should be published retained to MQTT.
    pub async fn start(
        config: &Config,
        postgres: sqlx::PgPool,
        message_sink: stateless::Sender<Message>,
    ) -> (Self, stateful::Receiver<Vec<ReminderInfo>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let (list_tx, list_rx) = stateful::create_pipe("reminders");

        // Clear any retained reminder list from a previous process before
        // publishing real state.
        list_tx.try_send(Vec::new());

        let silences = match load_active_silences(&postgres).await {
            Ok(silences) => silences,
            Err(err) => {
                error!("Failed to load reminder silences: {err}");
                Vec::new()
            }
        };

        let runner = Runner {
            postgres,
            message_sink,
            list_tx,
            silences,
            entries: HashMap::new(),
        };

        let interval = config.repeat_interval();
        spawn(async move {
            runner.run(rx, interval).await;
        });

        (Self { tx }, list_rx)
    }

    /// Create a new reminder.
    ///
    /// The returned handle must be kept alive for as long as the reminder
    /// should exist. The `key` is a stable identifier used to match
    /// silences.
    #[must_use]
    pub fn create(&self, key: impl Into<String>, message: Message) -> Reminder {
        let id = ReminderId::new(Uuid::new_v4().to_string());
        self.send(Command::Create {
            id: id.clone(),
            key: key.into(),
            message,
        });
        Reminder {
            id,
            tx: self.tx.clone(),
        }
    }

    /// Handle a command received from the frontend.
    pub fn handle_command(&self, command: ReminderCommand) {
        let command = match command {
            ReminderCommand::Acknowledge { id } => Command::Acknowledge(id),
            ReminderCommand::Silence {
                key,
                duration_minutes,
            } => Command::Silence {
                key,
                duration_minutes,
            },
            ReminderCommand::Unsilence { key } => Command::Unsilence { key },
        };
        self.send(command);
    }

    fn send(&self, command: Command) {
        if self.tx.send(command).is_err() {
            error!("Reminder service is no longer running");
        }
    }
}

struct Runner {
    postgres: sqlx::PgPool,
    message_sink: stateless::Sender<Message>,
    list_tx: stateful::Sender<Vec<ReminderInfo>>,
    silences: Vec<Silence>,
    entries: HashMap<ReminderId, Entry>,
}

impl Runner {
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Command>, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // The first tick completes immediately; skip it so a fresh start
        // does not send anything before reminders exist.
        ticker.tick().await;

        loop {
            tokio::select! {
                command = rx.recv() => {
                    let Some(command) = command else {
                        debug!("Reminder service command channel closed");
                        break;
                    };
                    self.handle_command(command).await;
                }
                _ = ticker.tick() => {
                    self.tick().await;
                }
            }
        }
    }

    async fn tick(&mut self) {
        self.refresh_silences().await;

        if self.apply_silences() {
            self.publish();
        }

        self.send_active();
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::Create { id, key, message } => {
                self.refresh_silences().await;
                let now = utc_now();
                let silenced = find_silence(&self.silences, &key, now);
                let entry = Entry {
                    id: id.clone(),
                    key,
                    message,
                    created_at: now,
                    acknowledged: false,
                    silenced,
                };

                if entry.status() == ReminderStatus::Active {
                    debug!(reminder_id = %id, "Sending new reminder");
                    self.message_sink.try_send(entry.message.clone());
                }

                self.entries.insert(id, entry);
                self.publish();
            }
            Command::Update { id, message } => {
                let changed = if let Some(entry) = self.entries.get_mut(&id) {
                    let changed = entry.update_message(message);
                    if changed && entry.status() == ReminderStatus::Active {
                        debug!(reminder_id = %id, "Sending updated reminder");
                        self.message_sink.try_send(entry.message.clone());
                    }
                    changed
                } else {
                    warn!(reminder_id = %id, "Ignoring message update of unknown reminder");
                    false
                };
                if changed {
                    self.publish();
                }
            }
            Command::Delete(id) => {
                if self.entries.remove(&id).is_some() {
                    debug!(reminder_id = %id, "Deleted reminder");
                    self.publish();
                }
            }
            Command::Acknowledge(id) => {
                let changed = match self.entries.get_mut(&id) {
                    Some(entry) if !entry.acknowledged => {
                        entry.acknowledged = true;
                        true
                    }
                    Some(_) => false,
                    None => {
                        warn!(reminder_id = %id, "Ignoring acknowledgement of unknown reminder");
                        false
                    }
                };
                if changed {
                    debug!(reminder_id = %id, "Acknowledged reminder");
                    self.publish();
                }
            }
            Command::Silence {
                key,
                duration_minutes,
            } => {
                let expires_at = duration_minutes
                    .map(|minutes| Utc::now() + chrono::Duration::minutes(i64::from(minutes)));
                if let Err(err) = insert_silence(&self.postgres, &key, expires_at, None).await {
                    error!("Failed to insert silence for {key}: {err}");
                    return;
                }
                info!(reminder_key = %key, ?expires_at, "Silenced reminder key");
                self.refresh_silences().await;
                self.apply_silences();
                self.publish();
            }
            Command::Unsilence { key } => {
                if let Err(err) = delete_silences_for_key(&self.postgres, &key).await {
                    error!("Failed to delete silences for {key}: {err}");
                    return;
                }
                info!(reminder_key = %key, "Unsilenced reminder key");
                self.refresh_silences().await;
                self.apply_silences();
                self.publish();
            }
        }
    }

    async fn refresh_silences(&mut self) {
        match load_active_silences(&self.postgres).await {
            Ok(silences) => self.silences = silences,
            Err(err) => error!("Failed to load reminder silences: {err}"),
        }
    }

    fn apply_silences(&mut self) -> bool {
        let now = utc_now();
        let mut changed = false;

        for entry in self.entries.values_mut() {
            let silenced = find_silence(&self.silences, &entry.key, now);
            if entry.silenced != silenced {
                entry.silenced = silenced;
                changed = true;
            }
        }

        changed
    }

    fn send_active(&self) {
        for entry in self.entries.values() {
            if entry.status() == ReminderStatus::Active {
                debug!(reminder_id = %entry.id, "Sending active reminder");
                self.message_sink.try_send(entry.message.clone());
            }
        }
    }

    fn publish(&self) {
        let mut infos = self.entries.values().map(Entry::info).collect::<Vec<_>>();
        infos.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        self.list_tx.try_send(infos);
    }
}

/// Find the longest-lasting active silence for a reminder key.
fn find_silence(silences: &[Silence], key: &str, now: DateTime<Utc>) -> Option<SilenceExpiry> {
    let mut result = None;

    for silence in silences {
        if silence.reminder_key != key {
            continue;
        }

        match silence.expires_at {
            None => return Some(SilenceExpiry::Indefinite),
            Some(expires_at) if expires_at > now => {
                result = Some(match result {
                    None | Some(SilenceExpiry::Indefinite) => SilenceExpiry::Until(expires_at),
                    Some(SilenceExpiry::Until(current)) => {
                        SilenceExpiry::Until(current.max(expires_at))
                    }
                });
            }
            Some(_) => {}
        }
    }

    result
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use robotica_common::robotica::message::MessagePriority;
    use robotica_tokio::pipes::{stateful, stateless, Subscriber, Subscription};
    use sqlx::PgPool;
    use std::collections::HashMap;

    fn silence(key: &str, expires_at: Option<DateTime<Utc>>) -> Silence {
        Silence {
            reminder_key: key.to_string(),
            expires_at,
        }
    }

    fn message(body: &str) -> Message {
        Message::new("Tesla", body, MessagePriority::Error, "audience")
    }

    fn entry(id: &str, message: Message) -> (ReminderId, Entry) {
        let id = ReminderId::new(id);
        (
            id.clone(),
            Entry {
                id,
                key: "tesla_1/doors".to_string(),
                message,
                created_at: utc_now(),
                acknowledged: false,
                silenced: None,
            },
        )
    }

    /// A runner wired to local pipes and a pool that is never connected.
    ///
    /// The update path never queries the database, so the lazy pool never
    /// establishes a connection.
    fn test_runner() -> (
        Runner,
        stateful::Receiver<Vec<ReminderInfo>>,
        stateless::Receiver<Message>,
    ) {
        let (message_tx, message_rx) = stateless::create_pipe("test_reminder_messages");
        let (list_tx, list_rx) = stateful::create_pipe("test_reminder_list");

        let postgres = PgPool::connect_lazy("postgres://localhost/robotica_test")
            .unwrap_or_else(|e| panic!("failed to create lazy pool: {e}"));

        let runner = Runner {
            postgres,
            message_sink: message_tx,
            list_tx,
            silences: Vec::new(),
            entries: HashMap::new(),
        };
        (runner, list_rx, message_rx)
    }

    #[test]
    fn test_entry_update_message() {
        let (_, mut entry) = entry("id", message("frunk is open"));

        // An identical message changes nothing.
        assert!(!entry.update_message(entry.message.clone()));
        assert_eq!(entry.status(), ReminderStatus::Active);
        assert_eq!(entry.message.body, "frunk is open");

        // A changed message resets acknowledgement.
        entry.acknowledged = true;
        assert!(entry.update_message(message("boot is open")));
        assert_eq!(entry.status(), ReminderStatus::Active);
        assert_eq!(entry.message.body, "boot is open");
    }

    #[tokio::test]
    async fn test_update_message_sends_when_active() {
        let (mut runner, list_rx, message_rx) = test_runner();
        let (id, entry) = entry("id", message("frunk is open"));
        runner.entries.insert(id.clone(), entry);

        let mut messages = message_rx.subscribe().await;
        let mut list = list_rx.subscribe().await;

        runner
            .handle_command(Command::Update {
                id: id.clone(),
                message: message("frunk and boot are open"),
            })
            .await;

        assert_eq!(
            messages.recv().await.unwrap().body,
            "frunk and boot are open"
        );

        let infos = list.recv().await.unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].id, id);
        assert_eq!(infos[0].message.body, "frunk and boot are open");
        assert_eq!(infos[0].status, ReminderStatus::Active);
    }

    #[tokio::test]
    async fn test_update_message_reactivates_acknowledged() {
        let (mut runner, list_rx, message_rx) = test_runner();
        let (id, mut entry) = entry("id", message("frunk is open"));
        entry.acknowledged = true;
        runner.entries.insert(id.clone(), entry);

        let mut messages = message_rx.subscribe().await;
        let mut list = list_rx.subscribe().await;

        runner
            .handle_command(Command::Update {
                id,
                message: message("frunk and boot are open"),
            })
            .await;

        assert_eq!(
            messages.recv().await.unwrap().body,
            "frunk and boot are open"
        );
        assert_eq!(list.recv().await.unwrap()[0].status, ReminderStatus::Active);
    }

    #[tokio::test]
    async fn test_update_message_same_message_is_noop() {
        let (mut runner, list_rx, message_rx) = test_runner();
        let (id, entry) = entry("id", message("frunk is open"));
        runner.entries.insert(id.clone(), entry);

        let mut messages = message_rx.subscribe().await;
        let mut list = list_rx.subscribe().await;

        runner
            .handle_command(Command::Update {
                id,
                message: message("frunk is open"),
            })
            .await;

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(messages.try_recv().unwrap().is_none());
        assert!(list.try_recv().unwrap().is_none());
    }

    #[tokio::test]
    async fn test_update_message_silenced_is_not_sent() {
        let (mut runner, list_rx, message_rx) = test_runner();
        let (id, mut entry) = entry("id", message("frunk is open"));
        entry.silenced = Some(SilenceExpiry::Indefinite);
        runner.entries.insert(id.clone(), entry);

        let mut messages = message_rx.subscribe().await;
        let mut list = list_rx.subscribe().await;

        runner
            .handle_command(Command::Update {
                id,
                message: message("frunk and boot are open"),
            })
            .await;

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(messages.try_recv().unwrap().is_none());

        let infos = list.recv().await.unwrap();
        assert_eq!(infos[0].message.body, "frunk and boot are open");
        assert_eq!(infos[0].status, ReminderStatus::Silenced);
    }

    #[test]
    fn test_find_silence_none() {
        let now = utc_now();
        let silences = vec![silence("other", None)];
        assert_eq!(find_silence(&silences, "key", now), None);
    }

    #[test]
    fn test_find_silence_indefinite() {
        let now = utc_now();
        let silences = vec![silence("key", None)];
        assert_eq!(
            find_silence(&silences, "key", now),
            Some(SilenceExpiry::Indefinite)
        );
    }

    #[test]
    fn test_find_silence_longest_expiry() {
        let now = utc_now();
        let first = now + chrono::Duration::hours(1);
        let second = now + chrono::Duration::hours(2);
        let silences = vec![silence("key", Some(first)), silence("key", Some(second))];
        assert_eq!(
            find_silence(&silences, "key", now),
            Some(SilenceExpiry::Until(second))
        );
    }

    #[test]
    fn test_find_silence_expired() {
        let now = utc_now();
        let silences = vec![silence("key", Some(now - chrono::Duration::hours(1)))];
        assert_eq!(find_silence(&silences, "key", now), None);
    }

    #[test]
    fn test_entry_status() {
        let entry = Entry {
            id: ReminderId::new("id"),
            key: "key".to_string(),
            message: Message::new("title", "body", MessagePriority::default(), "audience"),
            created_at: utc_now(),
            acknowledged: false,
            silenced: None,
        };
        assert_eq!(entry.status(), ReminderStatus::Active);

        let entry = Entry {
            silenced: Some(SilenceExpiry::Indefinite),
            ..entry
        };
        assert_eq!(entry.status(), ReminderStatus::Silenced);
        assert_eq!(entry.info().silenced_until, None);

        let entry = Entry {
            acknowledged: true,
            ..entry
        };
        assert_eq!(entry.status(), ReminderStatus::Acknowledged);
    }
}
