//! Connection management for WebSocket gateways.
//!
//! [`ConnectionManager`] tracks active connections, maps connection IDs to
//! sender channels, and supports room-based grouping for targeted broadcasts.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use tokio::sync::mpsc;
use uuid::Uuid;

/// Metadata about a single WebSocket connection.
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// Unique connection identifier.
    pub id: String,
    /// Optional user identifier associated with this connection.
    pub user_id: Option<String>,
    /// Set of room IDs this connection is subscribed to.
    pub rooms: HashSet<String>,
    /// When the connection was established.
    pub connected_at: Instant,
    /// When the last message was received from this connection.
    pub last_seen: Instant,
}

/// Manages active WebSocket connections and their room memberships.
///
/// Each connection is identified by a unique string ID and holds a
/// [`mpsc::UnboundedSender`] for outbound messages. Connections can be
/// grouped into named rooms for targeted broadcasting.
pub struct ConnectionManager {
    /// Sender half for each connection, keyed by connection ID.
    senders: HashMap<String, mpsc::UnboundedSender<String>>,
    /// Metadata for each connection.
    info: HashMap<String, ConnectionInfo>,
    /// Room memberships: room ID -> set of connection IDs.
    rooms: HashMap<String, HashSet<String>>,
}

impl ConnectionManager {
    /// Create a new, empty connection manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            senders: HashMap::new(),
            info: HashMap::new(),
            rooms: HashMap::new(),
        }
    }

    /// Register a new connection and return its unique ID and message receiver.
    ///
    /// The returned receiver yields serialized messages that should be sent over
    /// the WebSocket to the client.
    pub fn add(&mut self, user_id: Option<String>) -> (String, mpsc::UnboundedReceiver<String>) {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel();
        let now = Instant::now();

        let info = ConnectionInfo {
            id: id.clone(),
            user_id,
            rooms: HashSet::new(),
            connected_at: now,
            last_seen: now,
        };

        self.senders.insert(id.clone(), tx);
        self.info.insert(id.clone(), info);

        (id, rx)
    }

    /// Register a connection with a specific ID. Returns the message receiver.
    ///
    /// Useful when the caller wants to control the connection ID (e.g. for
    /// deterministic testing).
    pub fn add_with_id(
        &mut self,
        id: String,
        user_id: Option<String>,
    ) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded_channel();
        let now = Instant::now();

        let info = ConnectionInfo {
            id: id.clone(),
            user_id,
            rooms: HashSet::new(),
            connected_at: now,
            last_seen: now,
        };

        self.senders.insert(id.clone(), tx);
        self.info.insert(id, info);

        rx
    }

    /// Remove a connection by ID, cleaning up all room memberships.
    ///
    /// Returns the connection info if the connection existed.
    pub fn remove(&mut self, connection_id: &str) -> Option<ConnectionInfo> {
        self.senders.remove(connection_id);
        if let Some(info) = self.info.remove(connection_id) {
            // Remove from all rooms
            for room_id in &info.rooms {
                if let Some(members) = self.rooms.get_mut(room_id) {
                    members.remove(connection_id);
                    if members.is_empty() {
                        self.rooms.remove(room_id);
                    }
                }
            }
            Some(info)
        } else {
            None
        }
    }

    /// Get the connection info for a given ID.
    #[must_use]
    pub fn get(&self, connection_id: &str) -> Option<&ConnectionInfo> {
        self.info.get(connection_id)
    }

    /// Check whether a connection with the given ID exists.
    #[must_use]
    pub fn contains(&self, connection_id: &str) -> bool {
        self.info.contains_key(connection_id)
    }

    /// Return the number of active connections.
    #[must_use]
    pub fn count(&self) -> usize {
        self.info.len()
    }

    /// Return whether there are no active connections.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.info.is_empty()
    }

    /// List all connection IDs.
    pub fn connection_ids(&self) -> impl Iterator<Item = &str> {
        self.info.keys().map(String::as_str)
    }

    /// Add a connection to a room.
    ///
    /// Returns `true` if the connection was added to the room, `false` if it
    /// was already a member.
    pub fn join_room(&mut self, connection_id: &str, room_id: &str) -> bool {
        if let Some(info) = self.info.get_mut(connection_id) {
            if info.rooms.insert(room_id.to_owned()) {
                self.rooms
                    .entry(room_id.to_owned())
                    .or_default()
                    .insert(connection_id.to_owned());
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Remove a connection from a room.
    ///
    /// Returns `true` if the connection was removed, `false` if it was not a
    /// member.
    pub fn leave_room(&mut self, connection_id: &str, room_id: &str) -> bool {
        if let Some(info) = self.info.get_mut(connection_id) {
            if info.rooms.remove(room_id) {
                if let Some(members) = self.rooms.get_mut(room_id) {
                    members.remove(connection_id);
                    if members.is_empty() {
                        self.rooms.remove(room_id);
                    }
                }
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Get all connection IDs in a room.
    #[must_use]
    pub fn room_members(&self, room_id: &str) -> Option<&HashSet<String>> {
        self.rooms.get(room_id)
    }

    /// Get the number of connections in a room.
    #[must_use]
    pub fn room_count(&self, room_id: &str) -> usize {
        self.rooms.get(room_id).map_or(0, HashSet::len)
    }

    /// List all room IDs.
    pub fn room_ids(&self) -> impl Iterator<Item = &str> {
        self.rooms.keys().map(String::as_str)
    }

    /// Return the total number of rooms.
    #[must_use]
    pub fn total_rooms(&self) -> usize {
        self.rooms.len()
    }

    /// Send a text message to a specific connection.
    ///
    /// Returns `true` if the message was queued, `false` if the connection
    /// does not exist or the channel is closed.
    pub fn send_to(&self, connection_id: &str, message: &str) -> bool {
        if let Some(tx) = self.senders.get(connection_id) {
            tx.send(message.to_owned()).is_ok()
        } else {
            false
        }
    }

    /// Send a text message to all connections in a room.
    ///
    /// Returns the number of connections that received the message.
    pub fn send_to_room(&self, room_id: &str, message: &str) -> usize {
        let Some(members) = self.rooms.get(room_id) else {
            return 0;
        };
        let mut sent = 0;
        for conn_id in members {
            if self.send_to(conn_id, message) {
                sent += 1;
            }
        }
        sent
    }

    /// Send a text message to all connections in a room except the given one.
    ///
    /// Useful for relaying a message from one connection to all others in the
    /// same room.
    pub fn send_to_room_except(
        &self,
        room_id: &str,
        message: &str,
        except_id: &str,
    ) -> usize {
        let Some(members) = self.rooms.get(room_id) else {
            return 0;
        };
        let mut sent = 0;
        for conn_id in members {
            if conn_id != except_id && self.send_to(conn_id, message) {
                sent += 1;
            }
        }
        sent
    }

    /// Send a text message to all active connections.
    ///
    /// Returns the number of connections that received the message.
    pub fn broadcast(&self, message: &str) -> usize {
        let mut sent = 0;
        for tx in self.senders.values() {
            if tx.send(message.to_owned()).is_ok() {
                sent += 1;
            }
        }
        sent
    }

    /// Update the `last_seen` timestamp for a connection.
    pub fn touch(&mut self, connection_id: &str) {
        if let Some(info) = self.info.get_mut(connection_id) {
            info.last_seen = Instant::now();
        }
    }

    /// Find all connections whose `last_seen` is older than the given duration.
    #[must_use]
    pub fn stale_connections(&self, timeout: std::time::Duration) -> Vec<String> {
        let cutoff = Instant::now() - timeout;
        self.info
            .iter()
            .filter(|(_, info)| info.last_seen < cutoff)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Find all connections for a given user ID.
    #[must_use]
    pub fn connections_for_user(&self, user_id: &str) -> Vec<String> {
        self.info
            .iter()
            .filter(|(_, info)| info.user_id.as_deref() == Some(user_id))
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // ---- Construction ----

    #[test]
    fn new_manager_is_empty() {
        let mgr = ConnectionManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.count(), 0);
        assert_eq!(mgr.total_rooms(), 0);
    }

    #[test]
    fn default_is_same_as_new() {
        let mgr = ConnectionManager::default();
        assert!(mgr.is_empty());
        assert_eq!(mgr.count(), 0);
    }

    // ---- Add / Remove connections ----

    #[test]
    fn add_connection_increments_count() {
        let mut mgr = ConnectionManager::new();
        let (id, _rx) = mgr.add(None);
        assert!(!id.is_empty());
        assert_eq!(mgr.count(), 1);
        assert!(!mgr.is_empty());
    }

    #[test]
    fn add_multiple_connections() {
        let mut mgr = ConnectionManager::new();
        let (_id1, _rx1) = mgr.add(Some("user-a".into()));
        let (_id2, _rx2) = mgr.add(Some("user-b".into()));
        let (_id3, _rx3) = mgr.add(None);
        assert_eq!(mgr.count(), 3);
    }

    #[test]
    fn add_with_id_uses_given_id() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("my-conn-1".into(), Some("user-x".into()));
        assert!(mgr.contains("my-conn-1"));
        assert_eq!(mgr.count(), 1);
    }

    #[test]
    fn remove_connection_decrements_count() {
        let mut mgr = ConnectionManager::new();
        let (id, _rx) = mgr.add(None);
        assert_eq!(mgr.count(), 1);
        let info = mgr.remove(&id);
        assert!(info.is_some());
        assert_eq!(mgr.count(), 0);
        assert!(mgr.is_empty());
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let mut mgr = ConnectionManager::new();
        assert!(mgr.remove("ghost").is_none());
    }

    #[test]
    fn remove_cleans_up_room_membership() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), None);
        mgr.join_room("c-1", "room-a");
        mgr.join_room("c-1", "room-b");

        mgr.remove("c-1");
        assert_eq!(mgr.room_count("room-a"), 0);
        assert_eq!(mgr.room_count("room-b"), 0);
        assert_eq!(mgr.total_rooms(), 0);
    }

    #[test]
    fn remove_only_removes_target_from_shared_room() {
        let mut mgr = ConnectionManager::new();
        let _rx1 = mgr.add_with_id("c-1".into(), None);
        let _rx2 = mgr.add_with_id("c-2".into(), None);
        mgr.join_room("c-1", "shared");
        mgr.join_room("c-2", "shared");
        assert_eq!(mgr.room_count("shared"), 2);

        mgr.remove("c-1");
        assert_eq!(mgr.room_count("shared"), 1);
        assert!(mgr.room_members("shared").unwrap().contains("c-2"));
        assert_eq!(mgr.total_rooms(), 1);
    }

    // ---- Get / Contains ----

    #[test]
    fn get_returns_connection_info() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), Some("alice".into()));
        let info = mgr.get("c-1").unwrap();
        assert_eq!(info.id, "c-1");
        assert_eq!(info.user_id.as_deref(), Some("alice"));
        assert!(info.rooms.is_empty());
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let mgr = ConnectionManager::new();
        assert!(mgr.get("ghost").is_none());
    }

    #[test]
    fn contains_true_for_existing() {
        let mut mgr = ConnectionManager::new();
        let (id, _rx) = mgr.add(None);
        assert!(mgr.contains(&id));
    }

    #[test]
    fn contains_false_for_missing() {
        let mgr = ConnectionManager::new();
        assert!(!mgr.contains("missing"));
    }

    // ---- Connection IDs ----

    #[test]
    fn connection_ids_lists_all() {
        let mut mgr = ConnectionManager::new();
        let _rx1 = mgr.add_with_id("a".into(), None);
        let _rx2 = mgr.add_with_id("b".into(), None);
        let _rx3 = mgr.add_with_id("c".into(), None);
        let mut ids: Vec<&str> = mgr.connection_ids().collect();
        ids.sort();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    // ---- Room management ----

    #[test]
    fn join_room_adds_membership() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), None);
        assert!(mgr.join_room("c-1", "lobby"));
        assert_eq!(mgr.room_count("lobby"), 1);
        assert_eq!(mgr.total_rooms(), 1);

        let info = mgr.get("c-1").unwrap();
        assert!(info.rooms.contains("lobby"));
    }

    #[test]
    fn join_room_returns_false_for_duplicate() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), None);
        assert!(mgr.join_room("c-1", "lobby"));
        assert!(!mgr.join_room("c-1", "lobby"));
        assert_eq!(mgr.room_count("lobby"), 1);
    }

    #[test]
    fn join_room_nonexistent_connection_returns_false() {
        let mut mgr = ConnectionManager::new();
        assert!(!mgr.join_room("ghost", "lobby"));
        assert_eq!(mgr.total_rooms(), 0);
    }

    #[test]
    fn leave_room_removes_membership() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), None);
        mgr.join_room("c-1", "lobby");
        assert!(mgr.leave_room("c-1", "lobby"));
        assert_eq!(mgr.room_count("lobby"), 0);
        assert_eq!(mgr.total_rooms(), 0);

        let info = mgr.get("c-1").unwrap();
        assert!(!info.rooms.contains("lobby"));
    }

    #[test]
    fn leave_room_not_in_room_returns_false() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), None);
        assert!(!mgr.leave_room("c-1", "lobby"));
    }

    #[test]
    fn leave_room_nonexistent_connection_returns_false() {
        let mut mgr = ConnectionManager::new();
        assert!(!mgr.leave_room("ghost", "lobby"));
    }

    #[test]
    fn room_members_returns_correct_set() {
        let mut mgr = ConnectionManager::new();
        let _rx1 = mgr.add_with_id("c-1".into(), None);
        let _rx2 = mgr.add_with_id("c-2".into(), None);
        let _rx3 = mgr.add_with_id("c-3".into(), None);

        mgr.join_room("c-1", "room-a");
        mgr.join_room("c-2", "room-a");
        // c-3 not in room-a

        let members = mgr.room_members("room-a").unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.contains("c-1"));
        assert!(members.contains("c-2"));
        assert!(!members.contains("c-3"));
    }

    #[test]
    fn room_members_nonexistent_room_returns_none() {
        let mgr = ConnectionManager::new();
        assert!(mgr.room_members("ghost-room").is_none());
    }

    #[test]
    fn room_count_empty_room() {
        let mgr = ConnectionManager::new();
        assert_eq!(mgr.room_count("no-such-room"), 0);
    }

    #[test]
    fn room_ids_lists_all() {
        let mut mgr = ConnectionManager::new();
        let _rx1 = mgr.add_with_id("c-1".into(), None);
        let _rx2 = mgr.add_with_id("c-2".into(), None);
        mgr.join_room("c-1", "alpha");
        mgr.join_room("c-2", "beta");

        let mut ids: Vec<&str> = mgr.room_ids().collect();
        ids.sort();
        assert_eq!(ids, vec!["alpha", "beta"]);
    }

    #[test]
    fn connection_in_multiple_rooms() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), None);
        mgr.join_room("c-1", "room-a");
        mgr.join_room("c-1", "room-b");
        mgr.join_room("c-1", "room-c");

        let info = mgr.get("c-1").unwrap();
        assert_eq!(info.rooms.len(), 3);
        assert_eq!(mgr.total_rooms(), 3);
    }

    #[test]
    fn last_room_member_leaves_destroys_room() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), None);
        mgr.join_room("c-1", "ephemeral");
        assert_eq!(mgr.total_rooms(), 1);

        mgr.leave_room("c-1", "ephemeral");
        assert_eq!(mgr.total_rooms(), 0);
        assert!(mgr.room_members("ephemeral").is_none());
    }

    // ---- Send / Broadcast ----

    #[test]
    fn send_to_delivers_message() {
        let mut mgr = ConnectionManager::new();
        let mut rx = mgr.add_with_id("c-1".into(), None);

        assert!(mgr.send_to("c-1", "hello"));
        let msg = rx.try_recv().unwrap();
        assert_eq!(msg, "hello");
    }

    #[test]
    fn send_to_nonexistent_returns_false() {
        let mgr = ConnectionManager::new();
        assert!(!mgr.send_to("ghost", "hello"));
    }

    #[test]
    fn send_to_closed_receiver_returns_false() {
        let mut mgr = ConnectionManager::new();
        let rx = mgr.add_with_id("c-1".into(), None);
        drop(rx); // close the receiver
        assert!(!mgr.send_to("c-1", "hello"));
    }

    #[test]
    fn send_to_room_delivers_to_all_members() {
        let mut mgr = ConnectionManager::new();
        let mut rx1 = mgr.add_with_id("c-1".into(), None);
        let mut rx2 = mgr.add_with_id("c-2".into(), None);
        mgr.join_room("c-1", "room");
        mgr.join_room("c-2", "room");

        let sent = mgr.send_to_room("room", "event");
        assert_eq!(sent, 2);
        assert_eq!(rx1.try_recv().unwrap(), "event");
        assert_eq!(rx2.try_recv().unwrap(), "event");
    }

    #[test]
    fn send_to_room_nonexistent_returns_zero() {
        let mgr = ConnectionManager::new();
        assert_eq!(mgr.send_to_room("ghost", "msg"), 0);
    }

    #[test]
    fn send_to_room_except_excludes_sender() {
        let mut mgr = ConnectionManager::new();
        let mut rx1 = mgr.add_with_id("c-1".into(), None);
        let mut rx2 = mgr.add_with_id("c-2".into(), None);
        mgr.join_room("c-1", "room");
        mgr.join_room("c-2", "room");

        let sent = mgr.send_to_room_except("room", "relay", "c-1");
        assert_eq!(sent, 1);
        // c-2 got the message
        assert_eq!(rx2.try_recv().unwrap(), "relay");
        // c-1 did not
        assert!(rx1.try_recv().is_err());
    }

    #[test]
    fn send_to_room_except_nonexistent_room() {
        let mgr = ConnectionManager::new();
        assert_eq!(mgr.send_to_room_except("ghost", "msg", "nobody"), 0);
    }

    #[test]
    fn broadcast_to_all() {
        let mut mgr = ConnectionManager::new();
        let mut rx1 = mgr.add_with_id("c-1".into(), None);
        let mut rx2 = mgr.add_with_id("c-2".into(), None);
        let mut rx3 = mgr.add_with_id("c-3".into(), None);

        let sent = mgr.broadcast("global");
        assert_eq!(sent, 3);
        assert_eq!(rx1.try_recv().unwrap(), "global");
        assert_eq!(rx2.try_recv().unwrap(), "global");
        assert_eq!(rx3.try_recv().unwrap(), "global");
    }

    #[test]
    fn broadcast_to_empty_manager() {
        let mgr = ConnectionManager::new();
        assert_eq!(mgr.broadcast("nobody home"), 0);
    }

    #[test]
    fn broadcast_skips_closed_receivers() {
        let mut mgr = ConnectionManager::new();
        let mut rx1 = mgr.add_with_id("c-1".into(), None);
        let rx2 = mgr.add_with_id("c-2".into(), None);
        drop(rx2); // close c-2

        let sent = mgr.broadcast("partial");
        assert_eq!(sent, 1);
        assert_eq!(rx1.try_recv().unwrap(), "partial");
    }

    // ---- Touch / Stale ----

    #[test]
    fn touch_updates_last_seen() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), None);
        let before = mgr.get("c-1").unwrap().last_seen;

        // Spin briefly to ensure time advances
        std::thread::sleep(Duration::from_millis(10));
        mgr.touch("c-1");
        let after = mgr.get("c-1").unwrap().last_seen;
        assert!(after > before);
    }

    #[test]
    fn touch_nonexistent_is_noop() {
        let mut mgr = ConnectionManager::new();
        mgr.touch("ghost"); // should not panic
    }

    #[test]
    fn stale_connections_finds_old() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), None);

        // With a zero timeout, the connection is immediately stale
        let stale = mgr.stale_connections(Duration::ZERO);
        assert!(stale.contains(&"c-1".to_owned()));
    }

    #[test]
    fn stale_connections_excludes_fresh() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), None);
        mgr.touch("c-1");

        let stale = mgr.stale_connections(Duration::from_secs(60));
        assert!(stale.is_empty());
    }

    // ---- User lookups ----

    #[test]
    fn connections_for_user_finds_matches() {
        let mut mgr = ConnectionManager::new();
        let _rx1 = mgr.add_with_id("c-1".into(), Some("alice".into()));
        let _rx2 = mgr.add_with_id("c-2".into(), Some("alice".into()));
        let _rx3 = mgr.add_with_id("c-3".into(), Some("bob".into()));

        let mut alice_conns = mgr.connections_for_user("alice");
        alice_conns.sort();
        assert_eq!(alice_conns, vec!["c-1", "c-2"]);
    }

    #[test]
    fn connections_for_user_no_matches() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), Some("alice".into()));
        assert!(mgr.connections_for_user("carol").is_empty());
    }

    #[test]
    fn connections_for_user_skips_anonymous() {
        let mut mgr = ConnectionManager::new();
        let _rx1 = mgr.add_with_id("c-1".into(), None);
        let _rx2 = mgr.add_with_id("c-2".into(), Some("alice".into()));

        let conns = mgr.connections_for_user("alice");
        assert_eq!(conns, vec!["c-2"]);
    }

    // ---- Connection info fields ----

    #[test]
    fn connection_info_has_correct_user_id() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), Some("user-42".into()));
        assert_eq!(
            mgr.get("c-1").unwrap().user_id.as_deref(),
            Some("user-42")
        );
    }

    #[test]
    fn connection_info_anonymous_has_no_user_id() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), None);
        assert!(mgr.get("c-1").unwrap().user_id.is_none());
    }

    #[test]
    fn connection_info_rooms_reflect_joins() {
        let mut mgr = ConnectionManager::new();
        let _rx = mgr.add_with_id("c-1".into(), None);
        mgr.join_room("c-1", "r1");
        mgr.join_room("c-1", "r2");

        let info = mgr.get("c-1").unwrap();
        assert_eq!(info.rooms.len(), 2);
        assert!(info.rooms.contains("r1"));
        assert!(info.rooms.contains("r2"));
    }

    // ---- Multiple messages ----

    #[test]
    fn send_multiple_messages_in_order() {
        let mut mgr = ConnectionManager::new();
        let mut rx = mgr.add_with_id("c-1".into(), None);

        mgr.send_to("c-1", "first");
        mgr.send_to("c-1", "second");
        mgr.send_to("c-1", "third");

        assert_eq!(rx.try_recv().unwrap(), "first");
        assert_eq!(rx.try_recv().unwrap(), "second");
        assert_eq!(rx.try_recv().unwrap(), "third");
    }

    // ---- Overwrite connection with same ID ----

    #[test]
    fn add_with_id_overwrites_existing() {
        let mut mgr = ConnectionManager::new();
        let _rx1 = mgr.add_with_id("c-1".into(), Some("old-user".into()));
        let _rx2 = mgr.add_with_id("c-1".into(), Some("new-user".into()));

        assert_eq!(mgr.count(), 1);
        assert_eq!(
            mgr.get("c-1").unwrap().user_id.as_deref(),
            Some("new-user")
        );
    }

    // ---- Empty broadcast to room with disconnected member ----

    #[test]
    fn send_to_room_with_one_disconnected() {
        let mut mgr = ConnectionManager::new();
        let mut rx1 = mgr.add_with_id("c-1".into(), None);
        let rx2 = mgr.add_with_id("c-2".into(), None);
        mgr.join_room("c-1", "room");
        mgr.join_room("c-2", "room");

        drop(rx2);
        let sent = mgr.send_to_room("room", "test");
        // Only c-1 receives it
        assert_eq!(sent, 1);
        assert_eq!(rx1.try_recv().unwrap(), "test");
    }
}
