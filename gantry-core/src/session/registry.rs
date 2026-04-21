use anyhow::Result;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{Session, SessionHistory, SessionId};

/// Metadata about a session, derived from its filename.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub timestamp: Timestamp,
}

/// Abstracts session storage: create, load, and list sessions.
pub trait SessionRegistry {
    /// The history backend used by sessions this registry produces.
    type History: SessionHistory;

    /// Creates a new empty session with a fresh ID.
    fn create_session(&self) -> Result<Session<Self::History>>;

    /// Loads an existing session by ID.
    fn load_session(&self, session_id: &SessionId) -> Result<Session<Self::History>>;

    /// Lists all sessions, sorted by creation time (oldest first).
    fn list(&self) -> Result<Vec<SessionInfo>>;
}
