use chrono::{DateTime, Utc};
use dashmap::DashMap;
use dashmap::DashSet;

use crate::error::{DenshinError, Result};

/// A room that users can join to exchange events.
#[derive(Debug)]
pub struct Room {
    /// Unique room identifier.
    pub id: String,
    /// Human-readable room name.
    pub name: String,
    /// Set of user ids currently in the room.
    pub members: DashSet<String>,
    /// Optional cap on the number of members.
    pub max_members: Option<usize>,
    /// When the room was created.
    pub created_at: DateTime<Utc>,
}

impl Room {
    /// Creates a new room with the given id and name.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            members: DashSet::new(),
            max_members: None,
            created_at: Utc::now(),
        }
    }

    /// Creates a new room with a maximum member capacity.
    #[must_use]
    pub fn with_capacity(
        id: impl Into<String>,
        name: impl Into<String>,
        max_members: usize,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            members: DashSet::new(),
            max_members: Some(max_members),
            created_at: Utc::now(),
        }
    }

    /// Returns the current number of members.
    #[must_use]
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Returns `true` if the user is a member of this room.
    #[must_use]
    pub fn has_member(&self, user_id: &str) -> bool {
        self.members.contains(user_id)
    }
}

/// Manages a collection of rooms.
#[derive(Debug, Default)]
pub struct RoomManager {
    rooms: DashMap<String, Room>,
}

impl RoomManager {
    /// Creates a new empty room manager.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a room and inserts it into the manager.
    ///
    /// Returns the room id.
    #[must_use]
    pub fn create_room(&self, room: Room) -> String {
        let id = room.id.clone();
        self.rooms.insert(id.clone(), room);
        id
    }

    /// Adds a user to a room.
    ///
    /// # Errors
    ///
    /// Returns [`DenshinError::RoomNotFound`] if the room does not exist,
    /// or [`DenshinError::RoomFull`] if the room has reached capacity.
    pub fn join_room(&self, room_id: &str, user_id: &str) -> Result<()> {
        let room = self
            .rooms
            .get(room_id)
            .ok_or_else(|| DenshinError::RoomNotFound(room_id.into()))?;

        if let Some(max) = room.max_members
            && room.members.len() >= max
        {
            return Err(DenshinError::RoomFull(room_id.into()));
        }

        room.members.insert(user_id.into());
        Ok(())
    }

    /// Removes a user from a room.
    ///
    /// # Errors
    ///
    /// Returns [`DenshinError::RoomNotFound`] if the room does not exist.
    pub fn leave_room(&self, room_id: &str, user_id: &str) -> Result<()> {
        let room = self
            .rooms
            .get(room_id)
            .ok_or_else(|| DenshinError::RoomNotFound(room_id.into()))?;

        room.members.remove(user_id);
        Ok(())
    }

    /// Returns a list of member ids for the given room.
    ///
    /// # Errors
    ///
    /// Returns [`DenshinError::RoomNotFound`] if the room does not exist.
    pub fn get_room_members(&self, room_id: &str) -> Result<Vec<String>> {
        let room = self
            .rooms
            .get(room_id)
            .ok_or_else(|| DenshinError::RoomNotFound(room_id.into()))?;

        Ok(room.members.iter().map(|r| r.clone()).collect())
    }

    /// Returns a reference to a room if it exists.
    #[must_use]
    pub fn get_room(&self, room_id: &str) -> Option<dashmap::mapref::one::Ref<'_, String, Room>> {
        self.rooms.get(room_id)
    }

    /// Returns a list of all room ids.
    #[must_use]
    pub fn list_rooms(&self) -> Vec<String> {
        self.rooms.iter().map(|r| r.key().clone()).collect()
    }

    /// Broadcasts a serialised event payload to every member of a room.
    ///
    /// This is a helper that returns the list of member ids so the caller
    /// can deliver the message over their own transport layer. Denshin
    /// never touches the network directly.
    ///
    /// # Errors
    ///
    /// Returns [`DenshinError::RoomNotFound`] if the room does not exist.
    pub fn broadcast_to_room(&self, room_id: &str) -> Result<Vec<String>> {
        self.get_room_members(room_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_room() {
        let mgr = RoomManager::new();
        let room = Room::new("r-001", "General");
        let id = mgr.create_room(room);
        assert_eq!(id, "r-001");
        assert!(mgr.get_room("r-001").is_some());
    }

    #[test]
    fn join_room() {
        let mgr = RoomManager::new();
        let _ = mgr.create_room(Room::new("r-001", "General"));

        mgr.join_room("r-001", "u-001").unwrap();
        mgr.join_room("r-001", "u-002").unwrap();

        let members = mgr.get_room_members("r-001").unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.contains(&"u-001".to_string()));
        assert!(members.contains(&"u-002".to_string()));
    }

    #[test]
    fn join_nonexistent_room() {
        let mgr = RoomManager::new();
        let err = mgr.join_room("r-999", "u-001").unwrap_err();
        assert!(err.to_string().contains("room not found"));
    }

    #[test]
    fn leave_room() {
        let mgr = RoomManager::new();
        let _ = mgr.create_room(Room::new("r-001", "General"));
        mgr.join_room("r-001", "u-001").unwrap();

        mgr.leave_room("r-001", "u-001").unwrap();
        let members = mgr.get_room_members("r-001").unwrap();
        assert!(members.is_empty());
    }

    #[test]
    fn leave_nonexistent_room() {
        let mgr = RoomManager::new();
        let err = mgr.leave_room("r-999", "u-001").unwrap_err();
        assert!(err.to_string().contains("room not found"));
    }

    #[test]
    fn room_capacity_enforced() {
        let mgr = RoomManager::new();
        let _ = mgr.create_room(Room::with_capacity("r-001", "Small", 2));

        mgr.join_room("r-001", "u-001").unwrap();
        mgr.join_room("r-001", "u-002").unwrap();

        let err = mgr.join_room("r-001", "u-003").unwrap_err();
        assert!(err.to_string().contains("room full"));
    }

    #[test]
    fn list_rooms() {
        let mgr = RoomManager::new();
        let _ = mgr.create_room(Room::new("r-001", "General"));
        let _ = mgr.create_room(Room::new("r-002", "Random"));

        let rooms = mgr.list_rooms();
        assert_eq!(rooms.len(), 2);
        assert!(rooms.contains(&"r-001".to_string()));
        assert!(rooms.contains(&"r-002".to_string()));
    }

    #[test]
    fn broadcast_to_room_returns_members() {
        let mgr = RoomManager::new();
        let _ = mgr.create_room(Room::new("r-001", "General"));
        mgr.join_room("r-001", "u-001").unwrap();
        mgr.join_room("r-001", "u-002").unwrap();

        let targets = mgr.broadcast_to_room("r-001").unwrap();
        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn room_member_count() {
        let room = Room::new("r-001", "General");
        assert_eq!(room.member_count(), 0);
        room.members.insert("u-001".into());
        assert_eq!(room.member_count(), 1);
    }

    #[test]
    fn room_has_member() {
        let room = Room::new("r-001", "General");
        assert!(!room.has_member("u-001"));
        room.members.insert("u-001".into());
        assert!(room.has_member("u-001"));
    }
}
