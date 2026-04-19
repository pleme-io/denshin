use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Well-known event type constants used across the gateway.
pub struct EventType;

impl EventType {
    /// A new message was created.
    pub const MESSAGE_CREATE: &str = "message_create";
    /// A user's presence status changed.
    pub const PRESENCE_UPDATE: &str = "presence_update";
    /// A user joined a room.
    pub const ROOM_JOIN: &str = "room_join";
    /// A user left a room.
    pub const ROOM_LEAVE: &str = "room_leave";
    /// A user started typing.
    pub const TYPING_START: &str = "typing_start";
    /// An error occurred.
    pub const ERROR: &str = "error";
}

/// A gateway event envelope sent over the WebSocket connection.
///
/// This is a generic event structure — consumers (hiroba, taimen) deserialise
/// the [`payload`](GatewayEvent::payload) field into their domain-specific types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayEvent {
    /// The event type (see [`EventType`] constants).
    pub event_type: String,
    /// The room this event relates to, if any.
    pub room_id: Option<String>,
    /// The user who sent this event, if any.
    pub sender_id: Option<String>,
    /// Arbitrary JSON payload.
    pub payload: serde_json::Value,
    /// When the event was created.
    pub timestamp: DateTime<Utc>,
}

impl GatewayEvent {
    /// Creates a new gateway event with the current timestamp.
    #[must_use]
    pub fn new(
        event_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            room_id: None,
            sender_id: None,
            payload,
            timestamp: Utc::now(),
        }
    }

    /// Sets the room id on this event and returns it.
    #[must_use]
    pub fn with_room(mut self, room_id: impl Into<String>) -> Self {
        self.room_id = Some(room_id.into());
        self
    }

    /// Sets the sender id on this event and returns it.
    #[must_use]
    pub fn with_sender(mut self, sender_id: impl Into<String>) -> Self {
        self.sender_id = Some(sender_id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_roundtrip_json() {
        let event = GatewayEvent::new(EventType::MESSAGE_CREATE, json!({"text": "hello"}))
            .with_room("r-001")
            .with_sender("u-001");

        let json = serde_json::to_string(&event).unwrap();
        let decoded: GatewayEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn event_type_constants() {
        assert_eq!(EventType::MESSAGE_CREATE, "message_create");
        assert_eq!(EventType::PRESENCE_UPDATE, "presence_update");
        assert_eq!(EventType::ROOM_JOIN, "room_join");
        assert_eq!(EventType::ROOM_LEAVE, "room_leave");
        assert_eq!(EventType::TYPING_START, "typing_start");
        assert_eq!(EventType::ERROR, "error");
    }

    #[test]
    fn event_without_room_or_sender() {
        let event = GatewayEvent::new(EventType::ERROR, json!({"code": 500}));
        assert!(event.room_id.is_none());
        assert!(event.sender_id.is_none());

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"room_id\":null"));
        assert!(json.contains("\"sender_id\":null"));
    }

    #[test]
    fn event_serialises_event_type_field() {
        let event = GatewayEvent::new(EventType::TYPING_START, json!({}))
            .with_room("r-001")
            .with_sender("u-002");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event_type\":\"typing_start\""));
    }

    #[test]
    fn event_has_timestamp() {
        let before = Utc::now();
        let event = GatewayEvent::new(EventType::ROOM_JOIN, json!({}));
        let after = Utc::now();

        assert!(event.timestamp >= before);
        assert!(event.timestamp <= after);
    }

    #[test]
    fn event_builder_chain() {
        let event = GatewayEvent::new(EventType::ROOM_LEAVE, json!({"reason": "kicked"}))
            .with_room("r-005")
            .with_sender("u-010");

        assert_eq!(event.event_type, "room_leave");
        assert_eq!(event.room_id.as_deref(), Some("r-005"));
        assert_eq!(event.sender_id.as_deref(), Some("u-010"));
        assert_eq!(event.payload["reason"], "kicked");
    }
}
