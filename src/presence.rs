use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

/// Presence status for a user.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PresenceStatus {
    /// User is actively connected.
    Online,
    /// User is connected but inactive.
    Idle,
    /// User does not want to be disturbed.
    DoNotDisturb,
    /// User is disconnected.
    #[default]
    Offline,
}

/// Presence information for a single user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserPresence {
    /// The user this presence belongs to.
    pub user_id: String,
    /// Current presence status.
    pub status: PresenceStatus,
    /// When the user was last seen active.
    pub last_seen: DateTime<Utc>,
    /// Optional custom status message (e.g. "In a meeting").
    pub custom_status: Option<String>,
}

/// Tracks presence for all connected users.
#[derive(Debug, Default)]
pub struct PresenceTracker {
    presences: DashMap<String, UserPresence>,
}

impl PresenceTracker {
    /// Creates a new empty presence tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates the presence for a user. If the user has no existing
    /// presence entry, one is created.
    pub fn update(&self, user_id: &str, status: PresenceStatus, custom_status: Option<String>) {
        let presence = UserPresence {
            user_id: user_id.into(),
            status,
            last_seen: Utc::now(),
            custom_status,
        };
        self.presences.insert(user_id.into(), presence);
    }

    /// Returns the presence for a user, if tracked.
    #[must_use]
    pub fn get(&self, user_id: &str) -> Option<UserPresence> {
        self.presences.get(user_id).map(|r| r.clone())
    }

    /// Returns all users currently online (status is not [`PresenceStatus::Offline`]).
    #[must_use]
    pub fn get_online(&self) -> Vec<UserPresence> {
        self.presences
            .iter()
            .filter(|r| r.status != PresenceStatus::Offline)
            .map(|r| r.clone())
            .collect()
    }

    /// Removes the presence entry for a user.
    pub fn remove(&self, user_id: &str) {
        self.presences.remove(user_id);
    }

    /// Returns the total number of tracked users.
    #[must_use]
    pub fn len(&self) -> usize {
        self.presences.len()
    }

    /// Returns `true` if no presences are tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.presences.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_status_default_is_offline() {
        assert_eq!(PresenceStatus::default(), PresenceStatus::Offline);
    }

    #[test]
    fn presence_status_serialises_snake_case() {
        let json = serde_json::to_string(&PresenceStatus::DoNotDisturb).unwrap();
        assert_eq!(json, "\"do_not_disturb\"");
    }

    #[test]
    fn presence_status_roundtrip() {
        for status in [
            PresenceStatus::Online,
            PresenceStatus::Idle,
            PresenceStatus::DoNotDisturb,
            PresenceStatus::Offline,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let decoded: PresenceStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, decoded);
        }
    }

    #[test]
    fn update_and_get_presence() {
        let tracker = PresenceTracker::new();
        tracker.update("u-001", PresenceStatus::Online, None);

        let presence = tracker.get("u-001").unwrap();
        assert_eq!(presence.user_id, "u-001");
        assert_eq!(presence.status, PresenceStatus::Online);
        assert!(presence.custom_status.is_none());
    }

    #[test]
    fn update_overwrites_previous() {
        let tracker = PresenceTracker::new();
        tracker.update("u-001", PresenceStatus::Online, None);
        tracker.update(
            "u-001",
            PresenceStatus::DoNotDisturb,
            Some("In a meeting".into()),
        );

        let presence = tracker.get("u-001").unwrap();
        assert_eq!(presence.status, PresenceStatus::DoNotDisturb);
        assert_eq!(presence.custom_status.as_deref(), Some("In a meeting"));
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let tracker = PresenceTracker::new();
        assert!(tracker.get("u-999").is_none());
    }

    #[test]
    fn get_online_filters_offline() {
        let tracker = PresenceTracker::new();
        tracker.update("u-001", PresenceStatus::Online, None);
        tracker.update("u-002", PresenceStatus::Offline, None);
        tracker.update("u-003", PresenceStatus::Idle, None);
        tracker.update("u-004", PresenceStatus::DoNotDisturb, None);

        let online = tracker.get_online();
        assert_eq!(online.len(), 3);

        let ids: Vec<&str> = online.iter().map(|p| p.user_id.as_str()).collect();
        assert!(ids.contains(&"u-001"));
        assert!(ids.contains(&"u-003"));
        assert!(ids.contains(&"u-004"));
        assert!(!ids.contains(&"u-002"));
    }

    #[test]
    fn remove_presence() {
        let tracker = PresenceTracker::new();
        tracker.update("u-001", PresenceStatus::Online, None);
        assert_eq!(tracker.len(), 1);

        tracker.remove("u-001");
        assert!(tracker.get("u-001").is_none());
        assert!(tracker.is_empty());
    }

    #[test]
    fn user_presence_roundtrip_json() {
        let presence = UserPresence {
            user_id: "u-001".into(),
            status: PresenceStatus::Idle,
            last_seen: Utc::now(),
            custom_status: Some("afk".into()),
        };
        let json = serde_json::to_string(&presence).unwrap();
        let decoded: UserPresence = serde_json::from_str(&json).unwrap();
        assert_eq!(presence, decoded);
    }
}
