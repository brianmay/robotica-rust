use chrono::{DateTime, TimeDelta, Utc};
use opentelemetry::{global, metrics::Counter, KeyValue};
use robotica_backend::{
    pipes::{stateless, Subscriber, Subscription},
    services::{
        persistent_state,
        tesla::api::{self, CommandSequence, SequenceError, Token, VehicleId},
    },
    spawn,
};
use robotica_common::{
    robotica::{audio::MessagePriority, message::Message},
    unsafe_time_delta,
};
use std::time::Duration;
use std::{ops::Add, sync::Arc};
use thiserror::Error;
use tokio::{select, time::Instant};
use tracing::{error, info, instrument};

use crate::InitState;

use super::private::new_message;
use super::{Config, TeslamateAuth};

/// Errors that can occur when monitoring charging.
#[derive(Debug, Error)]
pub enum MonitorChargingError {
    /// An error occurred when loading the persistent state.
    #[error("failed to load persistent state: {0}")]
    LoadPersistentState(#[from] persistent_state::Error),
}

#[derive(Debug)]
struct Meters {
    api: api::Meters,
    outgoing_attempt: Counter<u64>,
    outgoing_requests: Counter<u64>,
    incoming_requests: Counter<u64>,
    notified_errors: Counter<u64>,
    cleared_errors: Counter<u64>,
    cancelled: Counter<u64>,
    vehicle_id: VehicleId,
}

#[derive(Debug, Copy, Clone)]
enum OutgoingStatus {
    Success,
    RateLimited,
    Error,
}

impl Meters {
    fn new(config: &Config) -> Self {
        let meter = global::meter("tesla::command_processor");
        Self {
            api: api::Meters::new(),
            outgoing_attempt: meter.u64_counter("outgoing_attempt").init(),
            outgoing_requests: meter.u64_counter("outgoing_requests").init(),
            incoming_requests: meter.u64_counter("incoming_requests").init(),
            notified_errors: meter.u64_counter("notified_errors").init(),
            cleared_errors: meter.u64_counter("cleared_errors").init(),
            cancelled: meter.u64_counter("cancelled").init(),
            vehicle_id: config.tesla_id,
        }
    }

    fn increment_cleared_errors(&self, forgotten: bool) {
        let attributes = [
            KeyValue::new("vehicle_id", self.vehicle_id.to_string()),
            KeyValue::new("forgotten", forgotten),
        ];
        self.cleared_errors.add(1, &attributes);
    }

    fn increment_notified_errors(&self) {
        let attributes = [KeyValue::new("vehicle_id", self.vehicle_id.to_string())];
        self.notified_errors.add(1, &attributes);
    }

    fn increment_incoming(&self, command: &Command, status: IncomingStatus) {
        let status = match status {
            IncomingStatus::Delayed => "delayed",
            IncomingStatus::Hurried => "hurried",
        };

        if !command.is_nil() {
            let attributes = [
                KeyValue::new("vehicle_id", self.vehicle_id.to_string()),
                KeyValue::new("charge_limit", format!("{:?}", command.charge_limit)),
                KeyValue::new("should_charge", format!("{:?}", command.should_charge)),
                KeyValue::new("status", status),
            ];
            self.incoming_requests.add(1, &attributes);
        }
    }

    fn increment_cancelled(&self) {
        let attributes = [KeyValue::new("vehicle_id", self.vehicle_id.to_string())];
        self.cancelled.add(1, &attributes);
    }

    fn increment_outgoing_started(&self, command: &Command) {
        if !command.is_nil() {
            let attributes = [
                KeyValue::new("vehicle_id", self.vehicle_id.to_string()),
                KeyValue::new("charge_limit", format!("{:?}", command.charge_limit)),
                KeyValue::new("should_charge", format!("{:?}", command.should_charge)),
            ];
            self.outgoing_attempt.add(1, &attributes);
        }
    }

    fn increment_outgoing_done(&self, command: &Command, status: OutgoingStatus) {
        let status = match status {
            OutgoingStatus::Success => "success",
            OutgoingStatus::RateLimited => "rate_limited",
            OutgoingStatus::Error => "error",
        };

        if !command.is_nil() {
            let attributes = [
                KeyValue::new("vehicle_id", self.vehicle_id.to_string()),
                KeyValue::new("charge_limit", format!("{:?}", command.charge_limit)),
                KeyValue::new("should_charge", format!("{:?}", command.should_charge)),
                KeyValue::new("status", status),
            ];
            self.outgoing_requests.add(1, &attributes);
        }
    }
}

#[derive(Debug, Clone)]
pub struct Command {
    charge_limit: Option<u8>,
    should_charge: Option<bool>,
}

impl Command {
    const fn is_nil(&self) -> bool {
        self.charge_limit.is_none() && self.should_charge.is_none()
    }

    #[must_use]
    pub const fn new() -> Self {
        Self {
            charge_limit: None,
            should_charge: None,
        }
    }

    #[must_use]
    pub const fn set_charge_limit(mut self, charge_limit: u8) -> Self {
        self.charge_limit = Some(charge_limit);
        self
    }

    #[must_use]
    pub const fn set_should_charge(mut self, should_charge: bool) -> Self {
        self.should_charge = Some(should_charge);
        self
    }
}

#[derive(Debug)]
struct TryCommand {
    command: Command,
    next_try_instant: Instant,
}

async fn sleep_until(maybe_try_command: &mut Option<TryCommand>) -> Option<TryCommand> {
    if let Some(try_command) = maybe_try_command {
        tokio::time::sleep_until(try_command.next_try_instant).await;
        maybe_try_command.take()
    } else {
        None
    }
}

#[derive(Debug)]
struct Errors {
    last_success: DateTime<Utc>,
    notified: bool,
}

impl Errors {
    fn new() -> Self {
        Self {
            last_success: Utc::now(),
            notified: false,
        }
    }

    fn forget_errors(&mut self, meters: &Meters) {
        if self.notified {
            meters.increment_cleared_errors(true);
        }
        self.last_success = Utc::now();
        self.notified = false;
    }

    fn notify_success(&mut self, message_sink: &stateless::Sender<Message>, meters: &Meters) {
        if self.notified {
            let msg = new_message(
                "I am on talking terms with the Tesla again",
                MessagePriority::Urgent,
            );
            message_sink.try_send(msg);
            meters.increment_cleared_errors(false);
        }
        self.last_success = Utc::now();
        self.notified = false;
    }

    const FAILURE_NOTIFICATION_INTERVAL: TimeDelta = unsafe_time_delta!(minutes: 30);

    fn notify_errors(&mut self, message_sink: &stateless::Sender<Message>, meters: &Meters) {
        if !self.notified && self.last_success.add(Self::FAILURE_NOTIFICATION_INTERVAL) < Utc::now()
        {
            meters.increment_notified_errors();
            let msg = new_message(
                "The Tesla and I have not been talking to each other for 30 minutes",
                MessagePriority::Urgent,
            );
            message_sink.try_send(msg);
            self.notified = true;
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum IncomingStatus {
    Delayed,
    Hurried,
}

pub fn run(
    state: &InitState,
    tesla: &Config,
    rx: stateless::Receiver<Command>,
    rx_token: stateless::Receiver<Arc<Token>>,
) {
    // let tesla_id = tesla.tesla_id;
    let name = tesla.name.clone();
    let tesla = tesla.clone();
    let message_sink = state.message_sink.clone();

    spawn(async move {
        let mut s = rx.subscribe().await;
        let mut s_token = rx_token.subscribe().await;

        let mut maybe_try_command: Option<TryCommand> = None;
        let mut errors = Errors::new();

        let tesla = tesla;
        let meters = Meters::new(&tesla);
        let Ok(mut token) = s_token.recv().await else {
            error!("Failed to get token.");
            return;
        };

        loop {
            select! {
                Some(try_command) = sleep_until(&mut maybe_try_command) => {
                    info!("{name}: Trying command: {:?}", try_command.command);
                    meters.increment_outgoing_started(&try_command.command);

                    match try_send(&try_command, &tesla, &token, &meters).await {
                        Ok(()) => {
                            meters.increment_outgoing_done(&try_command.command, OutgoingStatus::Success);
                            maybe_try_command = if try_command.command.is_nil() {
                                info!("{name}: Nil command succeeded.");
                                // If we didn't actually have a command, don't rate limit.
                                None
                                } else {
                                    info!("{name}: Command succeeded.");
                                    // If we did have a command, rate limit next command to 5 minutes.
                                    Some(TryCommand {
                                    command: Command::new(),
                                    next_try_instant: Instant::now() + Duration::from_secs(300),
                                })
                            };
                            errors.notify_success(&message_sink, &meters);
                        }
                        Err(SequenceError::WaitRetry(duration)) => {
                            info!("{name}: WaitRetry, retrying in {duration:?}.", );
                            meters.increment_outgoing_done(&try_command.command, OutgoingStatus::RateLimited);
                            maybe_try_command = Some(TryCommand {
                                command: try_command.command,
                                next_try_instant: Instant::now() + duration,
                            });

                            errors.notify_errors(&message_sink, &meters);
                        }
                        Err(err) => {
                            let duration = Duration::from_secs(60);
                            error!("{name}: Command failed: {err}, retrying in {duration:?}.");
                            meters.increment_outgoing_done(&try_command.command, OutgoingStatus::Error);
                            maybe_try_command = Some(TryCommand {
                                command: try_command.command,
                                next_try_instant: Instant::now() + duration,
                            });
                            errors.notify_errors(&message_sink, &meters);
                        }
                    }
                }
                Ok(command) = s.recv() => {
                    if let Some(try_command) = &maybe_try_command {
                        if !try_command.command.is_nil() && command.is_nil() {
                            meters.increment_cancelled();
                        }
                    }

                    if command.is_nil() {
                        info!("{name}: Received empty command: {:?}, forgetting errors.", command);
                        errors.forget_errors(&meters);
                    } else if maybe_try_command.is_none() {
                        // There may have been a large gap since we tried talking to the car
                        // last, hence we cannot rely on the last success time.
                        info!("{name}: Received command: {:?}, forgetting errors.", command);
                        errors.forget_errors(&meters);
                    }

                    let retry_time = match (&maybe_try_command, command.is_nil()) {
                        // We are rate limiting, so we need to keep the rate limit.
                        // Even if the command is nil.
                        (Some(try_command), _) => {
                            meters.increment_incoming(&command, IncomingStatus::Delayed);
                            Some(try_command.next_try_instant)
                        }

                        // We are not rate limiting and command is nil.
                        (None, true) => {
                            // We don't need to log nil commands.
                            None
                        }

                        // We are not rate limiting, command is not nil, execute immediately.
                        (None, false) => {
                            meters.increment_incoming(&command, IncomingStatus::Hurried);
                            Some(Instant::now())
                        }
                    };


                    if let Some(retry_time) = retry_time {
                        info!("{name}: Received command: {:?}, trying at {:?}.", command, retry_time - Instant::now());

                        maybe_try_command = Some(TryCommand {
                            command,
                            next_try_instant: retry_time,
                        });
                    } else {
                        info!("{name}: Received empty command: {:?}, ignoring.", command);
                    }
                }

                Ok(new_token) = s_token.recv() => {
                    info!("{name}: Received new token.");
                    token = new_token;
                }
            }
        }
    });
}

#[derive(Debug, Error)]
enum TeslamateError {
    #[error("Failed to enable logging: {0}")]
    Error(#[from] reqwest::Error),

    #[error("Failed to parse teslamate url: {0}")]
    ParseError(#[from] url::ParseError),
}

async fn enable_teslamate_logging(config: &Config) -> Result<(), TeslamateError> {
    let url = config.teslamate.url.join("/api/car/1/logging/resume")?;
    let client = reqwest::Client::new().put(url);
    let client = match &config.teslamate.auth {
        TeslamateAuth::Basic { username, password } => client.basic_auth(username, Some(password)),
        TeslamateAuth::None => client,
    };
    client.send().await?.error_for_status()?;
    Ok(())
}

#[instrument]
async fn try_send(
    try_command: &TryCommand,
    tesla: &Config,
    token: &Token,
    meters: &Meters,
) -> Result<(), SequenceError> {
    {
        let name = &tesla.name;

        // Construct sequence of commands to send to Tesla.
        let mut sequence = CommandSequence::new();

        // Wake up the car if it's not already awake.
        sequence.add_wake_up();

        if let Some(charge_limit) = try_command.command.charge_limit {
            sequence.add_set_chart_limit(charge_limit);
        }

        if let Some(should_charge) = try_command.command.should_charge {
            if should_charge {
                sequence.add_charge_start();
            } else {
                sequence.add_charge_stop();
            }
        }

        // Send the commands.
        info!("{name}: Sending commands: {sequence:?}");
        let result = sequence
            .execute(token, tesla.tesla_id, &meters.api)
            .await
            .map_err(|err| {
                info!("{name}: Error executing command sequence: {}", err);
                err
            });

        // If we attempted to change anything, ensure teslamate is logging so we get updates.
        if !sequence.is_empty() {
            // Any errors here should be logged and forgotten.
            if let Err(err) = enable_teslamate_logging(tesla).await {
                error!("{name}: Failed to enable teslamate logging: {}", err);
            }
        }

        info!("{name}: All done. {result:?}");

        result
    }
}
