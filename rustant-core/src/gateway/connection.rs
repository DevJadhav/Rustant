//! WebSocket connection management.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

/// Metadata about a connected WebSocket client.
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub connection_id: Uuid,
    pub authenticated: bool,
    pub connected_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
}

/// Manages active WebSocket connections.
#[derive(Debug, Default)]
pub struct ConnectionManager {
    connections: HashMap<Uuid, ConnectionInfo>,
    max_connections: usize,
}

impl ConnectionManager {
    /// Create a new connection manager with the given capacity limit.
    pub fn new(max_connections: usize) -> Self {
        Self {
            connections: HashMap::new(),
            max_connections,
        }
    }

    /// Register a new connection. Returns `None` if the limit is reached.
    pub fn add_connection(&mut self) -> Option<Uuid> {
        if self.connections.len() >= self.max_connections {
            return None;
        }

        let id = Uuid::new_v4();
        let now = Utc::now();
        self.connections.insert(
            id,
            ConnectionInfo {
                connection_id: id,
                authenticated: false,
                connected_at: now,
                last_activity: now,
            },
        );
        Some(id)
    }

    /// Remove a connection.
    pub fn remove_connection(&mut self, id: &Uuid) -> bool {
        self.connections.remove(id).is_some()
    }

    /// Mark a connection as authenticated.
    pub fn authenticate(&mut self, id: &Uuid) -> bool {
        if let Some(conn) = self.connections.get_mut(id) {
            conn.authenticated = true;
            conn.last_activity = Utc::now();
            true
        } else {
            false
        }
    }

    /// Update the last activity timestamp for a connection.
    pub fn touch(&mut self, id: &Uuid) {
        if let Some(conn) = self.connections.get_mut(id) {
            conn.last_activity = Utc::now();
        }
    }

    /// Get connection info.
    pub fn get(&self, id: &Uuid) -> Option<&ConnectionInfo> {
        self.connections.get(id)
    }

    /// Number of active connections.
    pub fn active_count(&self) -> usize {
        self.connections.len()
    }

    /// Number of authenticated connections.
    pub fn authenticated_count(&self) -> usize {
        self.connections
            .values()
            .filter(|c| c.authenticated)
            .count()
    }

    /// List all connection IDs.
    pub fn connection_ids(&self) -> Vec<Uuid> {
        self.connections.keys().copied().collect()
    }

    /// Check if a connection is authenticated.
    pub fn is_authenticated(&self, id: &Uuid) -> bool {
        self.connections
            .get(id)
            .map(|c| c.authenticated)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_connection() {
        let mut mgr = ConnectionManager::new(10);
        let id = mgr.add_connection();
        assert!(id.is_some());
        assert_eq!(mgr.active_count(), 1);
    }

    #[test]
    fn test_connection_limit() {
        let mut mgr = ConnectionManager::new(2);
        assert!(mgr.add_connection().is_some());
        assert!(mgr.add_connection().is_some());
        assert!(mgr.add_connection().is_none()); // limit reached
        assert_eq!(mgr.active_count(), 2);
    }

    #[test]
    fn test_remove_connection() {
        let mut mgr = ConnectionManager::new(10);
        let id = mgr.add_connection().unwrap();
        assert_eq!(mgr.active_count(), 1);

        assert!(mgr.remove_connection(&id));
        assert_eq!(mgr.active_count(), 0);

        // Removing nonexistent returns false
        assert!(!mgr.remove_connection(&Uuid::new_v4()));
    }

    #[test]
    fn test_authenticate_connection() {
        let mut mgr = ConnectionManager::new(10);
        let id = mgr.add_connection().unwrap();

        assert!(!mgr.is_authenticated(&id));
        assert_eq!(mgr.authenticated_count(), 0);

        assert!(mgr.authenticate(&id));
        assert!(mgr.is_authenticated(&id));
        assert_eq!(mgr.authenticated_count(), 1);
    }

    #[test]
    fn test_authenticate_nonexistent() {
        let mut mgr = ConnectionManager::new(10);
        assert!(!mgr.authenticate(&Uuid::new_v4()));
    }

    #[test]
    fn test_connection_ids() {
        let mut mgr = ConnectionManager::new(10);
        let id1 = mgr.add_connection().unwrap();
        let id2 = mgr.add_connection().unwrap();

        let ids = mgr.connection_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }

    #[test]
    fn test_get_connection_info() {
        let mut mgr = ConnectionManager::new(10);
        let id = mgr.add_connection().unwrap();

        let info = mgr.get(&id).unwrap();
        assert_eq!(info.connection_id, id);
        assert!(!info.authenticated);

        assert!(mgr.get(&Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_touch_updates_activity() {
        let mut mgr = ConnectionManager::new(10);
        let id = mgr.add_connection().unwrap();
        let initial = mgr.get(&id).unwrap().last_activity;

        // Touch to update
        mgr.touch(&id);
        let updated = mgr.get(&id).unwrap().last_activity;
        assert!(updated >= initial);
    }
}
