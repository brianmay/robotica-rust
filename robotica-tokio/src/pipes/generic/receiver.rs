//! Indexed receiver code.
//!
//! The generic pipe is the authoritative per-key store. [`into_stateful`] creates
//! a thick wrapper — it subscribes and forwards deduplicated values into a
//! [`stateful::create_indexed_pipe`]. [`into_stateless`] passes every message
//! through to a stateless pipe.

use async_trait::async_trait;
use robotica_common::mqtt::HasIndex;
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tracing::{debug, error};

use crate::{
    pipes::{stateful, stateless, Subscriber, Subscription as SubscriptionTrait},
    spawn,
};

use stateful::receiver::Subscription;

pub(in crate::pipes) use stateful::receiver::ReceiveMessage;

/// A `Receiver` that doesn't count as a reference to the entity.
pub struct WeakReceiver<T> {
    name: String,
    pub(in crate::pipes) tx: mpsc::WeakSender<ReceiveMessage<T>>,
}

impl<T> WeakReceiver<T> {
    /// Try to convert to a `Receiver`
    #[must_use]
    pub fn upgrade(&self) -> Option<Receiver<T>> {
        self.tx.upgrade().map(|tx| Receiver {
            name: self.name.clone(),
            tx,
        })
    }
}

/// Receive a value from an entity.
#[derive(Debug, Clone)]
pub struct Receiver<T> {
    pub(super) name: String,
    pub(in crate::pipes) tx: mpsc::Sender<ReceiveMessage<T>>,
}

#[async_trait]
impl<T> Subscriber<T> for Receiver<T>
where
    T: Send + Clone + 'static,
{
    type SubscriptionType = Subscription<T>;

    /// Subscribe to this entity.
    ///
    /// Returns an already closed subscription if the entity is closed.
    async fn subscribe(&self) -> Subscription<T> {
        let (tx, rx) = oneshot::channel();
        let msg = ReceiveMessage::Subscribe(tx);
        if let Err(err) = self.tx.send(msg).await {
            error!("{}: subscribe/send failed: {}", self.name, err);
            return Subscription::null(self.tx.clone());
        }
        rx.await.map_or_else(
            |_| {
                error!("{}: subscribe/await failed", self.name);
                Subscription::null(self.tx.clone())
            },
            |(rx, initial)| Subscription {
                rx,
                _tx: self.tx.clone(),
                initial,
            },
        )
    }
}

impl<T> Receiver<T> {
    /// Try to convert to a `WeakReceiver`
    #[must_use]
    pub fn downgrade(&self) -> WeakReceiver<T> {
        WeakReceiver {
            name: self.name.clone(),
            tx: self.tx.downgrade(),
        }
    }
}

impl<T> Receiver<T>
where
    T: Send + Clone,
{
    /// Retrieve the most recent value from the entity.
    ///
    /// Returns `None` if the entity is closed.
    pub async fn get(&self) -> Option<T> {
        let (tx, rx) = oneshot::channel();
        let msg = ReceiveMessage::Get(tx);
        if let Err(err) = self.tx.send(msg).await {
            error!("{}: get/send failed: {}", self.name, err);
            return None;
        }
        rx.await.unwrap_or_else(|_| {
            error!("{}: get/await failed", self.name);
            None
        })
    }
}

impl<T> Receiver<T>
where
    T: Send + Clone,
{
    /// Get a stateless receiver that forwards every message (new-value only).
    #[must_use]
    pub fn into_stateless(&self) -> stateless::Receiver<T>
    where
        T: 'static,
    {
        let name = format!("{} (into_stateless)", self.name);
        let (tx, rx) = stateless::create_pipe(&name);

        let clone_self = self.clone();

        spawn(async move {
            let mut sub = clone_self.subscribe().await;

            // Stateless should not replay initial data — clear it so only
            // new broadcast messages are forwarded.
            sub.initial.clear();

            loop {
                select! {
                    data = sub.recv() => {
                        let data = match data {
                            Ok(data) => data,
                            Err(err) => {
                                debug!("{name}: recv failed, exiting: {err}");
                                break;
                            }
                        };

                        tx.try_send(data);
                    }

                    () = tx.closed() => {
                        debug!("{name}: dest closed");
                        break;
                    }
                }
            }
        });
        rx
    }

    /// Turn this into a stateful indexed receiver.
    ///
    /// Subscribes to the generic pipe and forwards deduplicated per-key
    /// values into a [`stateful::create_indexed_pipe`].  The generic pipe's
    /// per-key `HashMap` ensures initial data is always correct.
    #[must_use]
    pub fn into_stateful(&self) -> stateful::Receiver<T>
    where
        T: PartialEq + HasIndex + 'static,
    {
        let name = format!("{} (into_stateful)", self.name);
        let (tx, rx) = stateful::create_indexed_pipe(&name);

        let clone_self = self.clone();

        spawn(async move {
            let mut sub = clone_self.subscribe().await;

            loop {
                select! {
                    data = sub.recv() => {
                        let data = match data {
                            Ok(data) => data,
                            Err(err) => {
                                debug!("{name}: recv failed, exiting: {err}");
                                break;
                            }
                        };

                        tx.try_send(data);
                    }

                    () = tx.closed() => {
                        debug!("{name}: dest closed");
                        break;
                    }
                }
            }
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::pipes::{generic, Subscriber};
    use robotica_common::mqtt::{MqttMessage, QoS, Retain};
    use std::time::Duration;

    fn msg(topic: &str, payload: &str) -> MqttMessage {
        MqttMessage::new(topic, payload, Retain::NoRetain, QoS::AtMostOnce)
    }

    async fn yield_twice() {
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
    }

    #[tokio::test]
    async fn test_stateless_receives_duplicates() {
        let (tx, rx) = generic::create_pipe("test");
        let stateless_rx = rx.into_stateless();
        let mut sub = stateless_rx.subscribe().await;

        tx.try_send(msg("topic/a", "hello"));
        yield_twice().await;

        let data = sub.recv().await.unwrap();
        assert_eq!(data.payload, b"hello");

        // Same key, same value — stateless should see it again
        tx.try_send(msg("topic/a", "hello"));
        yield_twice().await;

        let data = sub.recv().await.unwrap();
        assert_eq!(data.payload, b"hello");
    }

    #[tokio::test]
    async fn test_stateless_does_not_replay_initial() {
        let (tx, rx) = generic::create_pipe("test");

        // Send some messages BEFORE subscribing to stateless
        tx.try_send(msg("topic/a", "first"));
        tx.try_send(msg("topic/b", "second"));
        yield_twice().await;

        let stateless_rx = rx.into_stateless();
        let mut sub = stateless_rx.subscribe().await;

        // Initial data should have been cleared — nothing to receive
        assert!(sub.try_recv().unwrap().is_none());

        // New messages still arrive
        tx.try_send(msg("topic/c", "third"));
        yield_twice().await;

        let data = sub.recv().await.unwrap();
        assert_eq!(data.payload, b"third");
    }

    #[tokio::test]
    async fn test_stateful_replays_initial() {
        let (tx, rx) = generic::create_pipe("test");

        tx.try_send(msg("topic/a", "alpha"));
        tx.try_send(msg("topic/b", "beta"));
        yield_twice().await;

        let stateful_rx = rx.into_stateful();
        let mut sub = stateful_rx.subscribe().await;

        // Both per-key values should be replayed as initial state
        let first = sub.recv().await.unwrap();
        let second = sub.recv().await.unwrap();

        let mut payloads: Vec<String> = [first, second]
            .iter()
            .map(|m| String::from_utf8(m.payload.clone()).unwrap())
            .collect();
        payloads.sort();

        assert_eq!(payloads, vec!["alpha", "beta"]);
    }

    #[tokio::test]
    async fn test_stateful_deduplicates_same_key_value() {
        let (tx, rx) = generic::create_pipe("test");
        let stateful_rx = rx.into_stateful();
        let mut sub = stateful_rx.subscribe().await;

        tx.try_send(msg("topic/a", "hello"));
        yield_twice().await;

        let data = sub.recv().await.unwrap();
        assert_eq!(data.payload, b"hello");

        // Same key, same value — stateful should deduplicate
        tx.try_send(msg("topic/a", "hello"));
        yield_twice().await;

        assert!(sub.try_recv().unwrap().is_none());
    }

    #[tokio::test]
    async fn test_stateful_still_sees_changed_value() {
        let (tx, rx) = generic::create_pipe("test");
        let stateful_rx = rx.into_stateful();
        let mut sub = stateful_rx.subscribe().await;

        tx.try_send(msg("topic/a", "hello"));
        yield_twice().await;

        let data = sub.recv().await.unwrap();
        assert_eq!(data.payload, b"hello");

        // Different value — should be received
        tx.try_send(msg("topic/a", "world"));
        yield_twice().await;

        let data = sub.recv().await.unwrap();
        assert_eq!(data.payload, b"world");
    }

    #[tokio::test]
    async fn test_chatty_topic_does_not_evict_quiet_topic() {
        let (tx, rx) = generic::create_pipe("test");

        // Quiet topic sends one message
        tx.try_send(msg("quiet/topic", "important"));
        yield_twice().await;

        // Chatty topic sends many messages
        for i in 0..100 {
            tx.try_send(msg("chatty/topic", &format!("msg-{i}")));
        }
        yield_twice().await;

        // Subscribe via stateful — should still get the quiet topic's value
        let stateful_rx = rx.into_stateful();
        let mut sub = stateful_rx.subscribe().await;

        let mut found_quiet = false;
        let mut found_chatty = false;

        // Collect initial replay
        loop {
            match tokio::time::timeout(Duration::from_millis(50), sub.recv()).await {
                Ok(Ok(data)) => {
                    if data.payload == b"important" {
                        found_quiet = true;
                    }
                    if data.topic == "chatty/topic" {
                        found_chatty = true;
                    }
                }
                _ => break,
            }
        }

        assert!(found_quiet, "quiet topic was evicted by chatty topic");
        assert!(found_chatty, "chatty topic not found in replay");
    }

    #[tokio::test]
    async fn test_stateful_per_key_independent_state() {
        let (tx, rx) = generic::create_pipe("test");
        let stateful_rx = rx.into_stateful();
        let mut sub = stateful_rx.subscribe().await;

        // Topic A sends a value
        tx.try_send(msg("topic/a", "a1"));
        yield_twice().await;
        let data = sub.recv().await.unwrap();
        assert_eq!(data.payload, b"a1");

        // Topic B sends a value — topic A's state should be preserved
        tx.try_send(msg("topic/b", "b1"));
        yield_twice().await;

        // Receive the broadcast for B
        let mut saw_b1 = false;
        loop {
            match tokio::time::timeout(Duration::from_millis(50), sub.recv()).await {
                Ok(Ok(data)) => {
                    if data.payload == b"b1" {
                        saw_b1 = true;
                        break;
                    }
                }
                _ => break,
            }
        }
        assert!(saw_b1, "did not receive topic B's message");

        // Topic A sends the same value again — should be deduplicated
        tx.try_send(msg("topic/a", "a1"));
        yield_twice().await;
        assert!(sub.try_recv().unwrap().is_none());

        // Topic A sends a NEW value — should be received
        tx.try_send(msg("topic/a", "a2"));
        yield_twice().await;
        let data = sub.recv().await.unwrap();
        assert_eq!(data.payload, b"a2");
    }
}
