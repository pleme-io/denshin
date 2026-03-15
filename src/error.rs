//! Error types for denshin.

/// Errors that can occur during gateway operations.
#[derive(thiserror::Error, Debug)]
pub enum DenshinError {
    /// The specified connection was not found.
    #[error("connection not found: {0}")]
    ConnectionNotFound(String),

    /// The specified room was not found.
    #[error("room not found: {0}")]
    RoomNotFound(String),

    /// The connection is already in the specified room.
    #[error("connection {connection_id} already in room {room_id}")]
    AlreadyInRoom {
        /// The connection that is already in the room.
        connection_id: String,
        /// The room the connection is already in.
        room_id: String,
    },

    /// Failed to serialize a message.
    #[error("serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),

    /// Failed to send a message over a channel.
    #[error("send failed: channel closed")]
    SendFailed,

    /// The gateway has been shut down.
    #[error("gateway shut down")]
    Shutdown,
}

/// Convenience type alias for denshin results.
pub type Result<T> = std::result::Result<T, DenshinError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_not_found_display() {
        let err = DenshinError::ConnectionNotFound("conn-123".into());
        assert_eq!(format!("{err}"), "connection not found: conn-123");
    }

    #[test]
    fn room_not_found_display() {
        let err = DenshinError::RoomNotFound("room-abc".into());
        assert_eq!(format!("{err}"), "room not found: room-abc");
    }

    #[test]
    fn already_in_room_display() {
        let err = DenshinError::AlreadyInRoom {
            connection_id: "conn-1".into(),
            room_id: "room-1".into(),
        };
        assert_eq!(
            format!("{err}"),
            "connection conn-1 already in room room-1"
        );
    }

    #[test]
    fn serialize_error_from_serde() {
        let bad = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err: DenshinError = bad.into();
        let msg = format!("{err}");
        assert!(msg.starts_with("serialization failed:"));
    }

    #[test]
    fn send_failed_display() {
        let err = DenshinError::SendFailed;
        assert_eq!(format!("{err}"), "send failed: channel closed");
    }

    #[test]
    fn shutdown_display() {
        let err = DenshinError::Shutdown;
        assert_eq!(format!("{err}"), "gateway shut down");
    }

    #[test]
    fn error_is_debug() {
        let err = DenshinError::ConnectionNotFound("c-1".into());
        let debug = format!("{err:?}");
        assert!(debug.contains("ConnectionNotFound"));
    }

    #[test]
    fn result_type_alias_ok() {
        let result: Result<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn result_type_alias_err() {
        let result: Result<i32> = Err(DenshinError::Shutdown);
        assert!(result.is_err());
    }
}
