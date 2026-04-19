use tokio::sync::broadcast;

use crate::error::{DenshinError, Result};
use crate::event::GatewayEvent;

/// Fan-out event broadcaster backed by a [`tokio::broadcast`] channel.
///
/// All subscribers receive every event. Denshin does not filter by room —
/// consumers are expected to discard events not relevant to their context,
/// or use [`send_to_room`](Broadcaster::send_to_room) for room-scoped
/// pre-filtering via the event's `room_id` field.
#[derive(Debug)]
pub struct Broadcaster {
    sender: broadcast::Sender<GatewayEvent>,
}

impl Broadcaster {
    /// Creates a new broadcaster with the given channel capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Returns a new receiver that will receive all future events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
        self.sender.subscribe()
    }

    /// Sends an event to all current subscribers.
    ///
    /// # Errors
    ///
    /// Returns [`DenshinError::Broadcast`] if there are no active receivers.
    pub fn send(&self, event: GatewayEvent) -> Result<usize> {
        self.sender
            .send(event)
            .map_err(|e| DenshinError::Broadcast(e.to_string()))
    }

    /// Sends an event only if its `room_id` matches the given room.
    ///
    /// If the event's `room_id` is `None` or does not match, this is a no-op
    /// that returns `Ok(0)`.
    ///
    /// # Errors
    ///
    /// Returns [`DenshinError::Broadcast`] if the send fails.
    pub fn send_to_room(&self, event: GatewayEvent, room_id: &str) -> Result<usize> {
        if event.room_id.as_deref() == Some(room_id) {
            self.send(event)
        } else {
            Ok(0)
        }
    }

    /// Returns the number of active receivers.
    #[must_use]
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for Broadcaster {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventType;
    use serde_json::json;

    #[test]
    fn send_and_receive() {
        let broadcaster = Broadcaster::new(16);
        let mut rx = broadcaster.subscribe();

        let event = GatewayEvent::new(EventType::MESSAGE_CREATE, json!({"text": "hello"}))
            .with_room("r-001");

        let count = broadcaster.send(event.clone()).unwrap();
        assert_eq!(count, 1);

        let received = rx.try_recv().unwrap();
        assert_eq!(received, event);
    }

    #[test]
    fn multiple_subscribers() {
        let broadcaster = Broadcaster::new(16);
        let mut rx1 = broadcaster.subscribe();
        let mut rx2 = broadcaster.subscribe();

        let event = GatewayEvent::new(EventType::PRESENCE_UPDATE, json!({}));
        let count = broadcaster.send(event.clone()).unwrap();
        assert_eq!(count, 2);

        assert_eq!(rx1.try_recv().unwrap(), event);
        assert_eq!(rx2.try_recv().unwrap(), event);
    }

    #[test]
    fn send_with_no_receivers_fails() {
        let broadcaster = Broadcaster::new(16);
        let event = GatewayEvent::new(EventType::ERROR, json!({}));
        let err = broadcaster.send(event).unwrap_err();
        assert!(err.to_string().contains("broadcast error"));
    }

    #[test]
    fn send_to_room_matching() {
        let broadcaster = Broadcaster::new(16);
        let mut rx = broadcaster.subscribe();

        let event = GatewayEvent::new(EventType::TYPING_START, json!({})).with_room("r-001");
        let count = broadcaster.send_to_room(event.clone(), "r-001").unwrap();
        assert_eq!(count, 1);

        let received = rx.try_recv().unwrap();
        assert_eq!(received, event);
    }

    #[test]
    fn send_to_room_non_matching_is_noop() {
        let broadcaster = Broadcaster::new(16);
        let _rx = broadcaster.subscribe();

        let event = GatewayEvent::new(EventType::TYPING_START, json!({})).with_room("r-001");
        let count = broadcaster.send_to_room(event, "r-999").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn send_to_room_no_room_id_is_noop() {
        let broadcaster = Broadcaster::new(16);
        let _rx = broadcaster.subscribe();

        let event = GatewayEvent::new(EventType::ERROR, json!({}));
        let count = broadcaster.send_to_room(event, "r-001").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn receiver_count() {
        let broadcaster = Broadcaster::new(16);
        assert_eq!(broadcaster.receiver_count(), 0);

        let rx1 = broadcaster.subscribe();
        assert_eq!(broadcaster.receiver_count(), 1);

        let rx2 = broadcaster.subscribe();
        assert_eq!(broadcaster.receiver_count(), 2);

        drop(rx1);
        assert_eq!(broadcaster.receiver_count(), 1);
        drop(rx2);
    }

    #[test]
    fn default_capacity() {
        let broadcaster = Broadcaster::default();
        assert_eq!(broadcaster.receiver_count(), 0);

        // Verify default works by sending/receiving.
        let mut rx = broadcaster.subscribe();
        let event = GatewayEvent::new(EventType::ROOM_JOIN, json!({}));
        broadcaster.send(event.clone()).unwrap();
        assert_eq!(rx.try_recv().unwrap(), event);
    }
}
