use chrono::{DateTime, TimeDelta, Utc};
use robotica_backend::{
    pipes::{stateless, Subscriber, Subscription},
    services::{
        persistent_state::PersistentStateRow,
        tesla::api::{CommandSequence, SequenceError, Token, VehicleId},
    },
    spawn,
};
use robotica_common::{
    robotica::{audio::MessagePriority, message::Message},
    unsafe_time_delta,
};
use std::ops::Add;
use std::time::Duration;
use thiserror::Error;
use tokio::{select, time::Instant};
use tracing::{error, info};

use crate::InitState;

use super::{new_message, Config, MonitorChargingError, TeslamateAuth};

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
    send_left_home_commands: bool,
}

impl Errors {
    fn new() -> Self {
        Self {
            last_success: Utc::now(),
            notified: false,
            send_left_home_commands: false,
        }
    }

    fn forget_errors(&mut self) {
        self.last_success = Utc::now();
        self.notified = false;
        self.send_left_home_commands = false;
    }

    fn notify_success(&mut self, message_sink: &stateless::Sender<Message>) {
        if self.notified {
            let msg = new_message(
                "I am on talking terms with the Tesla again",
                MessagePriority::Urgent,
            );
            message_sink.try_send(msg);
        }
        self.forget_errors();
    }

    const FAILURE_NOTIFICATION_INTERVAL: TimeDelta = unsafe_time_delta!(minutes: 30);

    fn notify_errors(&mut self, message_sink: &stateless::Sender<Message>) {
        if !self.notified && self.last_success.add(Self::FAILURE_NOTIFICATION_INTERVAL) < Utc::now()
        {
            let msg = new_message(
                "The Tesla and I have not been talking to each other for 30 minutes",
                MessagePriority::Urgent,
            );
            message_sink.try_send(msg);
            self.notified = true;
        }
    }
}

pub fn run_command_processor(
    state: &InitState,
    tesla: &Config,
    rx: stateless::Receiver<Command>,
) -> Result<(), MonitorChargingError> {
    // let tesla_id = tesla.tesla_id;
    let name = tesla.name.clone();
    let tesla = tesla.clone();
    let message_sink = state.message_sink.clone();

    let tesla_secret = state.persistent_state_database.for_name("tesla_token");
    let mut token = Token::get(&tesla_secret)?;

    spawn(async move {
        let mut s = rx.subscribe().await;
        let mut maybe_try_command: Option<TryCommand> = None;
        let mut refresh_token_timer = tokio::time::interval(Duration::from_secs(3600));
        let mut errors = Errors::new();

        let tesla = tesla;

        check_token(&mut token, &tesla_secret).await;
        test_tesla_api(&token, tesla.tesla_id).await;

        loop {
            select! {
                _ = refresh_token_timer.tick() => {
                    check_token(&mut token, &tesla_secret).await;
                }
                Some(try_command) = sleep_until(&mut maybe_try_command) => {
                    info!("{name}: Trying command: {:?}", try_command.command);
                    match try_send(&try_command, &tesla, &token).await {
                        Ok(()) => {
                            maybe_try_command = if try_command.command.is_nil() {
                                info!("{name}: Nil command succeeded.");
                                // If we didn't actually have a command, don't rate limit.
                                None
                            } else {
                                info!("{name}: Command succeeded.");
                                // If we did have a command, rate limit next command to 5 minutes.
                                Some(TryCommand {
                                    command: try_command.command,
                                    next_try_instant: Instant::now() + Duration::from_secs(300),
                                })
                            };
                            errors.notify_success(&message_sink);
                        }
                        Err(SequenceError::WaitRetry(duration)) => {
                            info!("{name}: WaitRetry, retrying in {duration:?}.", );
                            maybe_try_command = Some(TryCommand {
                                command: try_command.command,
                                next_try_instant: Instant::now() + duration,
                            });
                            errors.notify_errors(&message_sink);
                        }
                        Err(err) => {
                            let duration = Duration::from_secs(60);
                            error!("{name}: Command failed: {err}, retrying in {duration:?}.");
                            maybe_try_command = Some(TryCommand {
                                command: try_command.command,
                                next_try_instant: Instant::now() + duration,
                            });
                            errors.notify_errors(&message_sink);
                        }
                    }
                }
                Ok(command) = s.recv() => {
                    if command.is_nil() {
                        info!("{name}: Received empty command: {:?}, forgetting errors.", command);
                        errors.forget_errors();
                    }

                    let retry_time = match (&maybe_try_command, command.is_nil()) {
                        // We are rate limiting, so we need to keep the rate limit.
                        // Even if the command is nil.
                        (Some(try_command), _) => Some(try_command.next_try_instant),

                        // We are not rate limiting and command is nil.
                        (None, true) => None,

                        // We are not rate limiting, command is not nil, execute immediately.
                        (None, false) => Some(Instant::now()),
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
            }
        }
    });

    Ok(())
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

pub async fn check_token(token: &mut Token, tesla_secret: &PersistentStateRow<Token>) {
    info!("Refreshing state, token expiration: {:?}", token.expires_at);
    token.check(tesla_secret).await.unwrap_or_else(|err| {
        error!("Failed to refresh token: {}", err);
    });
    info!("Token expiration: {:?}", token.expires_at);
}

async fn test_tesla_api(token: &Token, tesla_id: VehicleId) {
    let data = match token.get_vehicles().await {
        Ok(data) => data,
        Err(err) => {
            error!("Failed to get vehicles: {}", err);
            return;
        }
    };

    _ = data
        .into_iter()
        .find(|vehicle| vehicle.id == tesla_id)
        .ok_or_else(|| anyhow::anyhow!("Tesla vehicle {id} not found", id = tesla_id.to_string()));
}

async fn try_send(
    try_command: &TryCommand,
    tesla: &Config,
    token: &Token,
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
            .execute(token, tesla.tesla_id)
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
