//! High-level WebSocket gateway.
//!
//! [`WsGateway`] composes a [`ConnectionManager`] and an [`EventBroadcaster`]
//! behind an `Arc<RwLock>` so it can be shared across tasks (e.g. axum state).

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::{RwLock, broadcast, mpsc};

use crate::broadcaster::{BroadcasterConfig, EventBroadcaster};
use crate::connection::{ConnectionInfo, ConnectionManager};
use crate::error::Result;

/// Configuration for a [`WsGateway`].
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Broadcast channel capacity.
    pub broadcast_capacity: usize,
    /// Duration after which a connection is considered stale if it has not
    /// sent a heartbeat.
    pub heartbeat_timeout: Duration,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            broadcast_capacity: 1024,
            heartbeat_timeout: Duration::from_secs(60),
        }
    }
}

/// A thread-safe WebSocket gateway combining connection management and
/// event broadcasting.
///
/// Clone is cheap -- all clones share the same underlying state.
#[derive(Clone)]
pub struct WsGateway {
    connections: Arc<RwLock<ConnectionManager>>,
    broadcaster: EventBroadcaster,
    config: GatewayConfig,
}

impl WsGateway {
    /// Create a new gateway with the default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(GatewayConfig::default())
    }

    /// Create a new gateway with the given configuration.
    #[must_use]
    pub fn with_config(config: GatewayConfig) -> Self {
        let broadcaster = EventBroadcaster::with_config(BroadcasterConfig {
            capacity: config.broadcast_capacity,
        });
        Self {
            connections: Arc::new(RwLock::new(ConnectionManager::new())),
            broadcaster,
            config,
        }
    }

    /// Register a new connection. Returns the connection ID and a receiver
    /// for outbound messages.
    pub async fn connect(
        &self,
        user_id: Option<String>,
    ) -> (String, mpsc::UnboundedReceiver<String>) {
        self.connections.write().await.add(user_id)
    }

    /// Register a connection with a specific ID.
    pub async fn connect_with_id(
        &self,
        id: String,
        user_id: Option<String>,
    ) -> mpsc::UnboundedReceiver<String> {
        self.connections.write().await.add_with_id(id, user_id)
    }

    /// Disconnect a connection by ID, cleaning up room memberships.
    pub async fn disconnect(&self, connection_id: &str) -> Option<ConnectionInfo> {
        self.connections.write().await.remove(connection_id)
    }

    /// Get a snapshot of connection info.
    pub async fn connection_info(&self, connection_id: &str) -> Option<ConnectionInfo> {
        self.connections.read().await.get(connection_id).cloned()
    }

    /// Check whether a connection exists.
    pub async fn is_connected(&self, connection_id: &str) -> bool {
        self.connections.read().await.contains(connection_id)
    }

    /// Return the number of active connections.
    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.count()
    }

    /// Join a room.
    pub async fn join_room(&self, connection_id: &str, room_id: &str) -> bool {
        self.connections
            .write()
            .await
            .join_room(connection_id, room_id)
    }

    /// Leave a room.
    pub async fn leave_room(&self, connection_id: &str, room_id: &str) -> bool {
        self.connections
            .write()
            .await
            .leave_room(connection_id, room_id)
    }

    /// Get the member set for a room.
    pub async fn room_members(&self, room_id: &str) -> Option<HashSet<String>> {
        self.connections
            .read()
            .await
            .room_members(room_id)
            .cloned()
    }

    /// Get the number of connections in a room.
    pub async fn room_count(&self, room_id: &str) -> usize {
        self.connections.read().await.room_count(room_id)
    }

    /// Send a message directly to a specific connection.
    pub async fn send_to(&self, connection_id: &str, message: &str) -> bool {
        self.connections.read().await.send_to(connection_id, message)
    }

    /// Send a message to all connections in a room.
    pub async fn send_to_room(&self, room_id: &str, message: &str) -> usize {
        self.connections.read().await.send_to_room(room_id, message)
    }

    /// Send a message to all connections in a room except one.
    pub async fn send_to_room_except(
        &self,
        room_id: &str,
        message: &str,
        except_id: &str,
    ) -> usize {
        self.connections
            .read()
            .await
            .send_to_room_except(room_id, message, except_id)
    }

    /// Broadcast a message to all connections.
    pub async fn broadcast_raw(&self, message: &str) -> usize {
        self.connections.read().await.broadcast(message)
    }

    /// Publish a typed event on the broadcast channel.
    pub fn publish<E: Serialize>(&self, event: &E) -> Result<usize> {
        self.broadcaster.publish(event)
    }

    /// Publish a raw string on the broadcast channel.
    pub fn publish_raw(&self, message: &str) -> Result<usize> {
        self.broadcaster.publish_raw(message)
    }

    /// Subscribe to the broadcast channel.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.broadcaster.subscribe()
    }

    /// Update the heartbeat timestamp for a connection.
    pub async fn heartbeat(&self, connection_id: &str) {
        self.connections.write().await.touch(connection_id);
    }

    /// Find connections that have exceeded the heartbeat timeout.
    pub async fn stale_connections(&self) -> Vec<String> {
        self.connections
            .read()
            .await
            .stale_connections(self.config.heartbeat_timeout)
    }

    /// Find all connections for a given user.
    pub async fn connections_for_user(&self, user_id: &str) -> Vec<String> {
        self.connections
            .read()
            .await
            .connections_for_user(user_id)
    }

    /// Return the gateway configuration.
    #[must_use]
    pub fn config(&self) -> &GatewayConfig {
        &self.config
    }
}

impl Default for WsGateway {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Construction ----

    #[tokio::test]
    async fn new_gateway_has_no_connections() {
        let gw = WsGateway::new();
        assert_eq!(gw.connection_count().await, 0);
    }

    #[test]
    fn default_config_values() {
        let config = GatewayConfig::default();
        assert_eq!(config.broadcast_capacity, 1024);
        assert_eq!(config.heartbeat_timeout, Duration::from_secs(60));
    }

    #[test]
    fn custom_config() {
        let config = GatewayConfig {
            broadcast_capacity: 256,
            heartbeat_timeout: Duration::from_secs(30),
        };
        let gw = WsGateway::with_config(config);
        assert_eq!(gw.config().broadcast_capacity, 256);
        assert_eq!(gw.config().heartbeat_timeout, Duration::from_secs(30));
    }

    #[test]
    fn default_is_same_as_new() {
        let gw = WsGateway::default();
        assert_eq!(gw.config().broadcast_capacity, 1024);
    }

    // ---- Connect / Disconnect ----

    #[tokio::test]
    async fn connect_returns_id_and_receiver() {
        let gw = WsGateway::new();
        let (id, _rx) = gw.connect(Some("alice".into())).await;
        assert!(!id.is_empty());
        assert_eq!(gw.connection_count().await, 1);
        assert!(gw.is_connected(&id).await);
    }

    #[tokio::test]
    async fn connect_with_specific_id() {
        let gw = WsGateway::new();
        let _rx = gw
            .connect_with_id("my-id".into(), Some("bob".into()))
            .await;
        assert!(gw.is_connected("my-id").await);
    }

    #[tokio::test]
    async fn disconnect_removes_connection() {
        let gw = WsGateway::new();
        let (id, _rx) = gw.connect(None).await;
        let info = gw.disconnect(&id).await;
        assert!(info.is_some());
        assert!(!gw.is_connected(&id).await);
        assert_eq!(gw.connection_count().await, 0);
    }

    #[tokio::test]
    async fn disconnect_nonexistent_returns_none() {
        let gw = WsGateway::new();
        assert!(gw.disconnect("ghost").await.is_none());
    }

    // ---- Connection info ----

    #[tokio::test]
    async fn connection_info_returns_metadata() {
        let gw = WsGateway::new();
        let _rx = gw
            .connect_with_id("c-1".into(), Some("alice".into()))
            .await;
        let info = gw.connection_info("c-1").await.unwrap();
        assert_eq!(info.id, "c-1");
        assert_eq!(info.user_id.as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn connection_info_nonexistent_returns_none() {
        let gw = WsGateway::new();
        assert!(gw.connection_info("ghost").await.is_none());
    }

    // ---- Rooms ----

    #[tokio::test]
    async fn join_and_leave_room() {
        let gw = WsGateway::new();
        let _rx = gw.connect_with_id("c-1".into(), None).await;

        assert!(gw.join_room("c-1", "lobby").await);
        assert_eq!(gw.room_count("lobby").await, 1);

        assert!(gw.leave_room("c-1", "lobby").await);
        assert_eq!(gw.room_count("lobby").await, 0);
    }

    #[tokio::test]
    async fn room_members_snapshot() {
        let gw = WsGateway::new();
        let _rx1 = gw.connect_with_id("c-1".into(), None).await;
        let _rx2 = gw.connect_with_id("c-2".into(), None).await;

        gw.join_room("c-1", "room").await;
        gw.join_room("c-2", "room").await;

        let members = gw.room_members("room").await.unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.contains("c-1"));
        assert!(members.contains("c-2"));
    }

    #[tokio::test]
    async fn room_members_nonexistent_room() {
        let gw = WsGateway::new();
        assert!(gw.room_members("ghost").await.is_none());
    }

    // ---- Direct messaging ----

    #[tokio::test]
    async fn send_to_specific_connection() {
        let gw = WsGateway::new();
        let mut rx = gw.connect_with_id("c-1".into(), None).await;

        assert!(gw.send_to("c-1", "hello").await);
        assert_eq!(rx.recv().await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn send_to_nonexistent() {
        let gw = WsGateway::new();
        assert!(!gw.send_to("ghost", "hello").await);
    }

    // ---- Room messaging ----

    #[tokio::test]
    async fn send_to_room_delivers_to_members() {
        let gw = WsGateway::new();
        let mut rx1 = gw.connect_with_id("c-1".into(), None).await;
        let mut rx2 = gw.connect_with_id("c-2".into(), None).await;
        gw.join_room("c-1", "chat").await;
        gw.join_room("c-2", "chat").await;

        let sent = gw.send_to_room("chat", "event").await;
        assert_eq!(sent, 2);
        assert_eq!(rx1.recv().await.unwrap(), "event");
        assert_eq!(rx2.recv().await.unwrap(), "event");
    }

    #[tokio::test]
    async fn send_to_room_except_excludes_sender() {
        let gw = WsGateway::new();
        let mut rx1 = gw.connect_with_id("c-1".into(), None).await;
        let mut rx2 = gw.connect_with_id("c-2".into(), None).await;
        gw.join_room("c-1", "chat").await;
        gw.join_room("c-2", "chat").await;

        let sent = gw.send_to_room_except("chat", "relay", "c-1").await;
        assert_eq!(sent, 1);
        assert_eq!(rx2.recv().await.unwrap(), "relay");
        assert!(rx1.try_recv().is_err());
    }

    // ---- Global broadcast ----

    #[tokio::test]
    async fn broadcast_raw_to_all() {
        let gw = WsGateway::new();
        let mut rx1 = gw.connect_with_id("c-1".into(), None).await;
        let mut rx2 = gw.connect_with_id("c-2".into(), None).await;

        let sent = gw.broadcast_raw("alert").await;
        assert_eq!(sent, 2);
        assert_eq!(rx1.recv().await.unwrap(), "alert");
        assert_eq!(rx2.recv().await.unwrap(), "alert");
    }

    // ---- Pub/Sub broadcaster ----

    #[tokio::test]
    async fn publish_and_subscribe() {
        let gw = WsGateway::new();
        let mut sub = gw.subscribe();

        let count = gw.publish_raw("event-data").unwrap();
        assert_eq!(count, 1);

        let msg = sub.recv().await.unwrap();
        assert_eq!(msg, "event-data");
    }

    #[tokio::test]
    async fn publish_typed_event() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Debug)]
        struct MyEvent {
            kind: String,
        }

        let gw = WsGateway::new();
        let mut sub = gw.subscribe();

        let event = MyEvent {
            kind: "test".into(),
        };
        gw.publish(&event).unwrap();

        let msg = sub.recv().await.unwrap();
        let decoded: MyEvent = serde_json::from_str(&msg).unwrap();
        assert_eq!(decoded, event);
    }

    // ---- Heartbeat ----

    #[tokio::test]
    async fn heartbeat_updates_timestamp() {
        let gw = WsGateway::new();
        let _rx = gw.connect_with_id("c-1".into(), None).await;

        let before = gw.connection_info("c-1").await.unwrap().last_seen;
        tokio::time::sleep(Duration::from_millis(10)).await;
        gw.heartbeat("c-1").await;
        let after = gw.connection_info("c-1").await.unwrap().last_seen;
        assert!(after > before);
    }

    #[tokio::test]
    async fn stale_connections_detected() {
        let config = GatewayConfig {
            broadcast_capacity: 16,
            heartbeat_timeout: Duration::ZERO,
        };
        let gw = WsGateway::with_config(config);
        let _rx = gw.connect_with_id("c-1".into(), None).await;

        // With zero timeout, everything is stale immediately
        let stale = gw.stale_connections().await;
        assert!(stale.contains(&"c-1".to_owned()));
    }

    #[tokio::test]
    async fn fresh_connections_not_stale() {
        let gw = WsGateway::new(); // default 60s timeout
        let _rx = gw.connect_with_id("c-1".into(), None).await;
        gw.heartbeat("c-1").await;

        let stale = gw.stale_connections().await;
        assert!(stale.is_empty());
    }

    // ---- User lookups ----

    #[tokio::test]
    async fn connections_for_user() {
        let gw = WsGateway::new();
        let _rx1 = gw
            .connect_with_id("c-1".into(), Some("alice".into()))
            .await;
        let _rx2 = gw
            .connect_with_id("c-2".into(), Some("alice".into()))
            .await;
        let _rx3 = gw
            .connect_with_id("c-3".into(), Some("bob".into()))
            .await;

        let mut alice = gw.connections_for_user("alice").await;
        alice.sort();
        assert_eq!(alice, vec!["c-1", "c-2"]);
    }

    // ---- Clone shares state ----

    #[tokio::test]
    async fn cloned_gateway_shares_state() {
        let gw1 = WsGateway::new();
        let gw2 = gw1.clone();

        let _rx = gw1.connect_with_id("c-1".into(), None).await;
        assert_eq!(gw2.connection_count().await, 1);
        assert!(gw2.is_connected("c-1").await);
    }

    // ---- Disconnect cleans up rooms ----

    #[tokio::test]
    async fn disconnect_cleans_up_rooms() {
        let gw = WsGateway::new();
        let _rx = gw.connect_with_id("c-1".into(), None).await;
        gw.join_room("c-1", "room-a").await;
        gw.join_room("c-1", "room-b").await;

        gw.disconnect("c-1").await;
        assert_eq!(gw.room_count("room-a").await, 0);
        assert_eq!(gw.room_count("room-b").await, 0);
    }

    // ---- Multiple rooms, multiple connections ----

    #[tokio::test]
    async fn complex_room_scenario() {
        let gw = WsGateway::new();
        let mut rx1 = gw.connect_with_id("c-1".into(), None).await;
        let mut rx2 = gw.connect_with_id("c-2".into(), None).await;
        let mut rx3 = gw.connect_with_id("c-3".into(), None).await;

        // c-1 in room-a and room-b
        gw.join_room("c-1", "room-a").await;
        gw.join_room("c-1", "room-b").await;
        // c-2 in room-a only
        gw.join_room("c-2", "room-a").await;
        // c-3 in room-b only
        gw.join_room("c-3", "room-b").await;

        // Send to room-a: c-1 and c-2 get it
        let sent = gw.send_to_room("room-a", "msg-a").await;
        assert_eq!(sent, 2);
        assert_eq!(rx1.recv().await.unwrap(), "msg-a");
        assert_eq!(rx2.recv().await.unwrap(), "msg-a");
        assert!(rx3.try_recv().is_err());

        // Send to room-b: c-1 and c-3 get it
        let sent = gw.send_to_room("room-b", "msg-b").await;
        assert_eq!(sent, 2);
        assert_eq!(rx1.recv().await.unwrap(), "msg-b");
        assert_eq!(rx3.recv().await.unwrap(), "msg-b");
        assert!(rx2.try_recv().is_err());
    }
}
