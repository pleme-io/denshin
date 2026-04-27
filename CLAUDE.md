# Denshin (電信) -- Real-Time WebSocket Gateway

> **★★★ CSE / Knowable Construction.** This repo operates under **Constructive Substrate Engineering** — canonical specification at [`pleme-io/theory/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md`](https://github.com/pleme-io/theory/blob/main/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md). The Compounding Directive (operational rules: solve once, load-bearing fixes only, idiom-first, models stay current, direction beats velocity) is in the org-level pleme-io/CLAUDE.md ★★★ section. Read both before non-trivial changes.


## Build & Test

```bash
cargo build          # compile
cargo test           # all unit tests
```

## Architecture

Denshin is a **shared library** consumed by hiroba (chat server) and taimen
(video conferencing). It provides the common real-time infrastructure that
both products need: room management, event broadcasting, presence tracking,
and connection lifecycle.

### Module Map

| Module         | Purpose                                            | Key Types                                |
|----------------|----------------------------------------------------|------------------------------------------|
| `room.rs`      | Room state and membership                          | `Room`, `RoomManager`                    |
| `event.rs`     | Gateway event envelope and type constants          | `GatewayEvent`, `EventType`              |
| `presence.rs`  | Per-user presence tracking                         | `PresenceStatus`, `UserPresence`, `PresenceTracker` |
| `connection.rs`| Connection lifecycle and heartbeat detection       | `Connection`, `ConnectionState`          |
| `broadcast.rs` | Channel-based event fan-out                        | `Broadcaster`                            |
| `error.rs`     | Error types and Result alias                       | `DenshinError`, `Result`                 |

### Layer Position

```
 +-----------+     +-----------+
 |  hiroba   |     |  taimen   |
 |  (chat)   |     |  (video)  |
 +-----+-----+     +-----+-----+
       |                 |
       +--------+--------+
                |
          +-----+-----+
          |  denshin   |  <-- this crate
          |  (gateway) |
          +-----------+
```

Hiroba and taimen depend on denshin. Denshin does NOT depend on either.

### Design Decisions

- **Pure state, no I/O** -- denshin manages in-memory state (rooms, presence,
  connections) but never opens sockets, binds ports, or accepts connections.
  Consumers bring their own axum/tungstenite runtime and call denshin's API
  to mutate state and broadcast events.
- **DashMap for concurrency** -- rooms, presence, and connections use `DashMap`
  / `DashSet` for lock-free concurrent reads and fine-grained write locking.
- **tokio::broadcast for fan-out** -- the `Broadcaster` wraps a
  `tokio::broadcast` channel so multiple subscribers receive every event.
- **No persistence** -- all state is ephemeral. Consumers are responsible for
  persisting anything that must survive a restart.

## Consumers

- **hiroba** -- chat server (channels, messages, typing indicators)
- **taimen** -- video conferencing (rooms, participants, signaling)
