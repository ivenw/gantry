use anyhow::Result;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::message::Message;

use super::{Session, SessionHistory, SessionId};

/// Metadata about a session, derived from its file and first node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub timestamp: Timestamp,
    pub first_message: String,
}

/// Abstracts session storage: create, load, and list sessions.
pub trait SessionRegistry {
    /// The history backend used by sessions this registry produces.
    type History: SessionHistory;

    /// Creates a new session with `first_message` as its root node.
    fn create_session(&self, first_message: Message) -> Result<Session<Self::History>>;

    /// Loads an existing session by ID.
    fn load_session(&self, session_id: &SessionId) -> Result<Session<Self::History>>;

    /// Lists all sessions, sorted by creation time (oldest first).
    fn list(&self) -> Result<Vec<SessionInfo>>;
}
