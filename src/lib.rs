//! Denshin (電信) --- WebSocket gateway library.
//!
//! Provides connection management, room-based broadcasting, and event
//! multiplexing as reusable primitives for WebSocket servers.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use denshin::{WsGateway, GatewayConfig};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() {
//!     let gw = WsGateway::with_config(GatewayConfig {
//!         broadcast_capacity: 512,
//!         heartbeat_timeout: Duration::from_secs(30),
//!     });
//!
//!     // Register a connection
//!     let (conn_id, mut rx) = gw.connect(Some("user-1".into())).await;
//!
//!     // Join a room
//!     gw.join_room(&conn_id, "general").await;
//!
//!     // Send to all members of a room
//!     gw.send_to_room("general", r#"{"type":"message","text":"hello"}"#).await;
//!
//!     // Publish on the broadcast channel
//!     gw.publish_raw("global-event").unwrap();
//!
//!     // Disconnect
//!     gw.disconnect(&conn_id).await;
//! }
//! ```

pub mod broadcaster;
pub mod connection;
pub mod error;
pub mod gateway;

pub use broadcaster::{BroadcasterConfig, EventBroadcaster};
pub use connection::{ConnectionInfo, ConnectionManager};
pub use error::{DenshinError, Result};
pub use gateway::{GatewayConfig, WsGateway};
