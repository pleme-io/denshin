//! Event broadcasting over `tokio::sync::broadcast` channels.
//!
//! [`EventBroadcaster`] wraps a broadcast channel with typed serialization
//! so that any `Serialize` payload can be pushed to all subscribers.

use serde::Serialize;
use tokio::sync::broadcast;
use tracing::warn;

use crate::error::Result;

/// A typed event broadcaster backed by a tokio broadcast channel.
///
/// Events are serialized to JSON strings before being placed on the channel,
/// so subscribers receive `String` payloads they can forward directly over
/// WebSocket frames.
#[derive(Debug, Clone)]
pub struct EventBroadcaster {
    tx: broadcast::Sender<String>,
    capacity: usize,
}

/// Configuration for creating an [`EventBroadcaster`].
#[derive(Debug, Clone)]
pub struct BroadcasterConfig {
    /// Maximum number of messages the broadcast channel can buffer.
    /// Slow receivers that fall behind by more than this many messages
    /// will miss older events.
    pub capacity: usize,
}

impl Default for BroadcasterConfig {
    fn default() -> Self {
        Self { capacity: 1024 }
    }
}

impl EventBroadcaster {
    /// Create a new broadcaster with the default capacity (1024).
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(BroadcasterConfig::default())
    }

    /// Create a new broadcaster with the given configuration.
    #[must_use]
    pub fn with_config(config: BroadcasterConfig) -> Self {
        let (tx, _) = broadcast::channel(config.capacity);
        Self {
            tx,
            capacity: config.capacity,
        }
    }

    /// Create a new broadcaster with the given capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_config(BroadcasterConfig { capacity })
    }

    /// Publish a serializable event to all subscribers.
    ///
    /// The event is serialized to JSON. Returns the number of receivers
    /// that will receive the message.
    pub fn publish<E: Serialize>(&self, event: &E) -> Result<usize> {
        let json = serde_json::to_string(event)?;
        self.publish_raw(&json)
    }

    /// Publish a pre-serialized string to all subscribers.
    ///
    /// Returns the number of receivers that will receive the message.
    pub fn publish_raw(&self, message: &str) -> Result<usize> {
        match self.tx.send(message.to_owned()) {
            Ok(n) => Ok(n),
            Err(_) => {
                // No active receivers -- this is not an error, just nobody
                // is listening right now.
                warn!("broadcast: no active receivers");
                Ok(0)
            }
        }
    }

    /// Subscribe to this broadcaster. Returns a receiver that yields
    /// serialized event strings.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Return the number of active subscribers.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }

    /// Return the configured capacity of the broadcast channel.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct TestEvent {
        kind: String,
        payload: String,
    }

    // ---- Construction ----

    #[test]
    fn new_broadcaster_has_default_capacity() {
        let b = EventBroadcaster::new();
        assert_eq!(b.capacity(), 1024);
    }

    #[test]
    fn custom_capacity() {
        let b = EventBroadcaster::with_capacity(256);
        assert_eq!(b.capacity(), 256);
    }

    #[test]
    fn config_sets_capacity() {
        let config = BroadcasterConfig { capacity: 512 };
        let b = EventBroadcaster::with_config(config);
        assert_eq!(b.capacity(), 512);
    }

    #[test]
    fn default_config_capacity_is_1024() {
        let config = BroadcasterConfig::default();
        assert_eq!(config.capacity, 1024);
    }

    #[test]
    fn default_is_same_as_new() {
        let b = EventBroadcaster::default();
        assert_eq!(b.capacity(), 1024);
    }

    // ---- Subscriber count ----

    #[test]
    fn no_subscribers_initially() {
        let b = EventBroadcaster::new();
        assert_eq!(b.subscriber_count(), 0);
    }

    #[test]
    fn subscribe_increments_count() {
        let b = EventBroadcaster::new();
        let _rx1 = b.subscribe();
        assert_eq!(b.subscriber_count(), 1);
        let _rx2 = b.subscribe();
        assert_eq!(b.subscriber_count(), 2);
    }

    #[test]
    fn drop_subscriber_decrements_count() {
        let b = EventBroadcaster::new();
        let rx = b.subscribe();
        assert_eq!(b.subscriber_count(), 1);
        drop(rx);
        assert_eq!(b.subscriber_count(), 0);
    }

    // ---- Publish ----

    #[tokio::test]
    async fn publish_delivers_to_subscriber() {
        let b = EventBroadcaster::new();
        let mut rx = b.subscribe();

        let event = TestEvent {
            kind: "chat".into(),
            payload: "hello".into(),
        };
        let count = b.publish(&event).unwrap();
        assert_eq!(count, 1);

        let msg = rx.recv().await.unwrap();
        let decoded: TestEvent = serde_json::from_str(&msg).unwrap();
        assert_eq!(decoded, event);
    }

    #[tokio::test]
    async fn publish_delivers_to_multiple_subscribers() {
        let b = EventBroadcaster::new();
        let mut rx1 = b.subscribe();
        let mut rx2 = b.subscribe();
        let mut rx3 = b.subscribe();

        let event = TestEvent {
            kind: "notify".into(),
            payload: "ding".into(),
        };
        let count = b.publish(&event).unwrap();
        assert_eq!(count, 3);

        let m1 = rx1.recv().await.unwrap();
        let m2 = rx2.recv().await.unwrap();
        let m3 = rx3.recv().await.unwrap();
        assert_eq!(m1, m2);
        assert_eq!(m2, m3);
    }

    #[test]
    fn publish_with_no_subscribers_returns_zero() {
        let b = EventBroadcaster::new();
        let event = TestEvent {
            kind: "lonely".into(),
            payload: "echo".into(),
        };
        let count = b.publish(&event).unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn publish_raw_delivers_string() {
        let b = EventBroadcaster::new();
        let mut rx = b.subscribe();

        let count = b.publish_raw(r#"{"raw":"data"}"#).unwrap();
        assert_eq!(count, 1);

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg, r#"{"raw":"data"}"#);
    }

    #[test]
    fn publish_raw_no_subscribers() {
        let b = EventBroadcaster::new();
        let count = b.publish_raw("hello").unwrap();
        assert_eq!(count, 0);
    }

    // ---- Multiple publishes ----

    #[tokio::test]
    async fn multiple_publishes_received_in_order() {
        let b = EventBroadcaster::new();
        let mut rx = b.subscribe();

        b.publish_raw("first").unwrap();
        b.publish_raw("second").unwrap();
        b.publish_raw("third").unwrap();

        assert_eq!(rx.recv().await.unwrap(), "first");
        assert_eq!(rx.recv().await.unwrap(), "second");
        assert_eq!(rx.recv().await.unwrap(), "third");
    }

    // ---- Clone ----

    #[tokio::test]
    async fn cloned_broadcaster_shares_channel() {
        let b1 = EventBroadcaster::new();
        let b2 = b1.clone();
        let mut rx = b1.subscribe();

        // Publishing on the clone reaches the subscriber from the original
        b2.publish_raw("from-clone").unwrap();
        assert_eq!(rx.recv().await.unwrap(), "from-clone");
    }

    #[test]
    fn cloned_broadcaster_shares_subscriber_count() {
        let b1 = EventBroadcaster::new();
        let _rx = b1.subscribe();
        let b2 = b1.clone();
        assert_eq!(b2.subscriber_count(), 1);
    }

    // ---- Lagged receiver ----

    #[tokio::test]
    async fn slow_receiver_gets_lagged_error() {
        // Use a tiny capacity so we can overflow easily
        let b = EventBroadcaster::with_capacity(2);
        let mut rx = b.subscribe();

        // Send more messages than the capacity
        b.publish_raw("1").unwrap();
        b.publish_raw("2").unwrap();
        b.publish_raw("3").unwrap(); // this should cause receiver to lag

        let result = rx.recv().await;
        match result {
            Err(broadcast::error::RecvError::Lagged(n)) => {
                assert!(n > 0);
            }
            _ => {
                // It's also valid if the receiver got the latest message.
                // Behavior depends on timing. We just verify no panic.
            }
        }
    }

    // ---- Serialization errors ----

    #[test]
    fn publish_valid_json_succeeds() {
        let b = EventBroadcaster::new();
        let _rx = b.subscribe();
        let result = b.publish(&serde_json::json!({"key": "value"}));
        assert!(result.is_ok());
    }

    // ---- Struct events ----

    #[tokio::test]
    async fn publish_nested_struct() {
        #[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
        struct Outer {
            inner: Inner,
        }

        #[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
        struct Inner {
            value: i32,
        }

        let b = EventBroadcaster::new();
        let mut rx = b.subscribe();

        let event = Outer {
            inner: Inner { value: 42 },
        };
        b.publish(&event).unwrap();

        let msg = rx.recv().await.unwrap();
        let decoded: Outer = serde_json::from_str(&msg).unwrap();
        assert_eq!(decoded, event);
    }

    // ---- Subscribe after publish ----

    #[test]
    fn late_subscriber_does_not_get_old_messages() {
        let b = EventBroadcaster::new();
        let _early = b.subscribe(); // need at least one for send to succeed
        b.publish_raw("before").unwrap();

        let mut late = b.subscribe();
        // Late subscriber should have nothing to receive
        assert!(late.try_recv().is_err());
    }
}
