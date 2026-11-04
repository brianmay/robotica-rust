//! The pipe system: typed streams of values shared between tasks.
//!
//! Pipes are the backbone of message passing in robotica. Creating a pipe
//! returns a sender and a receiver; any number of tasks can
//! [`Subscriber::subscribe`] to a receiver and process values as they
//! arrive. Receivers also expose a functional combinator API (`map`,
//! `filter`, `for_each`, ...) for wiring streams together without a
//! hand-written task.
//!
//! Pipes come in two kinds, created with [`stateful::create_pipe`] or
//! [`stateless::create_pipe`]. The choice encodes what the values *are*,
//! not how often they change:
//!
//! * A **stateful** pipe holds the current *state* of an entity. New
//!   subscribers are immediately replayed the latest value — even if it is
//!   old — and values equal to the current one are deduplicated, so
//!   consumers are only woken when the state actually changes.
//! * A **stateless** pipe is an *event* stream. Every value is fired to the
//!   current subscribers and then forgotten: no storage, no replay, no
//!   deduplication. A consumer that is not subscribed at the moment a value
//!   is sent never sees it.
//!
//! # Which kind should I use?
//!
//! Ask: "if a consumer subscribes late, should it receive the most recent
//! value right away, even though it was published before subscribing?"
//!
//! * Yes — the value is *state*: use a stateful pipe. Examples: a sensor
//!   reading, the current Amber prices, a battery level, a switch position.
//!   The classic consumer pattern is to subscribe and then make one
//!   blocking [`Subscription::recv`] call to pick up the initial value
//!   before entering the event loop.
//! * No, it should just wait for the next occurrence — the value is an
//!   *event*: use a stateless pipe. Examples: a button press, a command, an
//!   interval boundary.
//!
//! A value that is different on every update is still state, not an event:
//! a temperature that changes with every reading is the current
//! temperature, and a late consumer wants it immediately.
//!
//! # Stateful details
//!
//! Deduplication compares values with `PartialEq`, so the equality of the
//! value type is part of the pipe's behaviour: state fields belong in
//! `PartialEq`, while a field that changes on every update (for example a
//! fetch timestamp) effectively disables deduplication for that stream.
//! Values are compared by content, so wrapping in `Arc` still compares the
//! inner value.
//!
//! # Publishing to MQTT
//!
//! The `send_to_mqtt*` helpers on receivers choose defaults that mirror the
//! state/event distinction:
//!
//! | Receiver kind | `QoS` default | Retain default |
//! |---------------|-------------|----------------|
//! | stateful | `AtLeastOnce` | `Retain` |
//! | stateless | `ExactlyOnce` | `NoRetain` |
//!
//! A retained message is replayed by the MQTT broker to every new subscriber
//! of the topic, doing for external consumers what the stateful replay does
//! for internal ones. Publishing a stateless receiver is fine, but if late
//! subscribers need the latest value, pass
//! `SendOptions::new().retain(Retain::Retain)` explicitly.
//!
//! See [`stateful`] and [`stateless`] for the details of each kind,
//! [`generic`] for a variant that stores the last value per key, and
//! [`delays`] for delaying and rate limiting values.

use async_trait::async_trait;
use thiserror::Error;

pub mod delays;
pub mod generic;
pub mod stateful;
pub mod stateless;

/// Size of all pipes.
pub const PIPE_SIZE: usize = 50;

/// Something went wrong in Receiver.
#[derive(Error, Debug)]
pub enum RecvError {
    /// The Pipe was closed.
    #[error("The pipe was closed")]
    Closed,
}

/// Allow subscribing to an pipe
#[async_trait]
pub trait Subscriber<T> {
    /// The type of the subscription
    type SubscriptionType: Subscription<T> + Send + 'static;

    /// Subscribe to a pipe
    ///
    /// Returns an already closed subscription if the entity is closed.
    async fn subscribe(&self) -> Self::SubscriptionType;
}

/// A subscription to a pipe
#[async_trait]
pub trait Subscription<T> {
    /// Wait for the next value from the entity
    ///
    /// This will return the new value.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    async fn recv(&mut self) -> Result<T, RecvError>;

    /// Get the next value but don't wait for it. Returns `None` if there is no value
    ///
    /// This will return the new value.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    fn try_recv(&mut self) -> Result<Option<T>, RecvError>;
}
