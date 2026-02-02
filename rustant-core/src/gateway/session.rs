//! Gateway session lifecycle management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// State of a gateway session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Active,
    Paused,
    Ended,
}

/// A gateway session representing an agent interaction.
#[derive(Debug, Clone)]
pub struct GatewaySession {
    pub session_id: Uuid,
    pub state: SessionState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub connection_id: Uuid,
}

/// Manages gateway sessions.
#[derive(Debug, Default)]
pub struct SessionManager {
    sessions: HashMap<Uuid, GatewaySession>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new session for a connection.
    pub fn create_session(&mut self, connection_id: Uuid) -> Uuid {
        let now = Utc::now();
        let session_id = Uuid::new_v4();
        self.sessions.insert(
            session_id,
            GatewaySession {
                session_id,
                state: SessionState::Active,
                created_at: now,
                updated_at: now,
                connection_id,
            },
        );
        session_id
    }

    /// Pause an active session.
    pub fn pause_session(&mut self, session_id: &Uuid) -> bool {
        if let Some(session) = self.sessions.get_mut(session_id) {
            if session.state == SessionState::Active {
                session.state = SessionState::Paused;
                session.updated_at = Utc::now();
                return true;
            }
        }
        false
    }

    /// Resume a paused session.
    pub fn resume_session(&mut self, session_id: &Uuid) -> bool {
        if let Some(session) = self.sessions.get_mut(session_id) {
            if session.state == SessionState::Paused {
                session.state = SessionState::Active;
                session.updated_at = Utc::now();
                return true;
            }
        }
        false
    }

    /// End a session.
    pub fn end_session(&mut self, session_id: &Uuid) -> bool {
        if let Some(session) = self.sessions.get_mut(session_id) {
            if session.state != SessionState::Ended {
                session.state = SessionState::Ended;
                session.updated_at = Utc::now();
                return true;
            }
        }
        false
    }

    /// Get session info.
    pub fn get(&self, session_id: &Uuid) -> Option<&GatewaySession> {
        self.sessions.get(session_id)
    }

    /// Count active sessions.
    pub fn active_count(&self) -> usize {
        self.sessions
            .values()
            .filter(|s| s.state == SessionState::Active)
            .count()
    }

    /// Total sessions (all states).
    pub fn total_count(&self) -> usize {
        self.sessions.len()
    }

    /// Remove ended sessions.
    pub fn cleanup_ended(&mut self) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| s.state != SessionState::Ended);
        before - self.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session() {
        let mut mgr = SessionManager::new();
        let conn_id = Uuid::new_v4();
        let session_id = mgr.create_session(conn_id);

        let session = mgr.get(&session_id).unwrap();
        assert_eq!(session.state, SessionState::Active);
        assert_eq!(session.connection_id, conn_id);
        assert_eq!(mgr.active_count(), 1);
    }

    #[test]
    fn test_pause_resume_session() {
        let mut mgr = SessionManager::new();
        let session_id = mgr.create_session(Uuid::new_v4());

        assert!(mgr.pause_session(&session_id));
        assert_eq!(mgr.get(&session_id).unwrap().state, SessionState::Paused);
        assert_eq!(mgr.active_count(), 0);

        assert!(mgr.resume_session(&session_id));
        assert_eq!(mgr.get(&session_id).unwrap().state, SessionState::Active);
        assert_eq!(mgr.active_count(), 1);
    }

    #[test]
    fn test_end_session() {
        let mut mgr = SessionManager::new();
        let session_id = mgr.create_session(Uuid::new_v4());

        assert!(mgr.end_session(&session_id));
        assert_eq!(mgr.get(&session_id).unwrap().state, SessionState::Ended);
        assert_eq!(mgr.active_count(), 0);

        // Can't end an already-ended session
        assert!(!mgr.end_session(&session_id));
    }

    #[test]
    fn test_invalid_state_transitions() {
        let mut mgr = SessionManager::new();
        let session_id = mgr.create_session(Uuid::new_v4());

        // Can't resume an active session
        assert!(!mgr.resume_session(&session_id));

        // Can't pause a paused session
        mgr.pause_session(&session_id);
        assert!(!mgr.pause_session(&session_id));
    }

    #[test]
    fn test_cleanup_ended() {
        let mut mgr = SessionManager::new();
        let s1 = mgr.create_session(Uuid::new_v4());
        let _s2 = mgr.create_session(Uuid::new_v4());
        let s3 = mgr.create_session(Uuid::new_v4());

        mgr.end_session(&s1);
        mgr.end_session(&s3);

        assert_eq!(mgr.total_count(), 3);
        let removed = mgr.cleanup_ended();
        assert_eq!(removed, 2);
        assert_eq!(mgr.total_count(), 1);
    }

    #[test]
    fn test_nonexistent_session() {
        let mut mgr = SessionManager::new();
        let fake = Uuid::new_v4();
        assert!(mgr.get(&fake).is_none());
        assert!(!mgr.pause_session(&fake));
        assert!(!mgr.resume_session(&fake));
        assert!(!mgr.end_session(&fake));
    }

    #[test]
    fn test_session_state_serialization() {
        let json = serde_json::to_string(&SessionState::Active).unwrap();
        let restored: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, SessionState::Active);
    }
}
