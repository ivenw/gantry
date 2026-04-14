use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::session::store::SessionStore;
use crate::types::{Message, Role};

/// In-memory session state for a single conversation tree.
///
/// All messages from the JSONL file are loaded into a HashMap keyed by message
/// ID. `current_leaf_id` tracks the tip of the active branch; new messages are
/// appended as children of that node.
pub struct SessionManager {
    pub session_id: String,
    /// The ID of the last message on the currently active branch.
    /// `None` only when the session has no messages yet.
    pub current_leaf_id: Option<String>,
    /// All messages in the tree, keyed by message ID.
    messages: HashMap<String, Message>,
    /// Retained so disk writes don't need the caller to pass it every time.
    project_path: PathBuf,
}

impl SessionManager {
    /// Create a fresh session on disk and return an empty manager.
    pub fn create(project_path: &Path) -> Result<Self> {
        let session_id = SessionStore::create(project_path)?;
        Ok(Self {
            session_id,
            current_leaf_id: None,
            messages: HashMap::new(),
            project_path: project_path.to_path_buf(),
        })
    }

    /// Load an existing session from disk into memory.
    ///
    /// `current_leaf_id` is initialised to the most recently created message
    /// that has no children (i.e. a leaf node with the latest `created_at`).
    pub fn load(project_path: &Path, session_id: &str) -> Result<Self> {
        let messages_vec = SessionStore::load_messages(project_path, session_id)
            .with_context(|| format!("failed to load session {}", session_id))?;

        let messages: HashMap<String, Message> = messages_vec
            .into_iter()
            .map(|m| (m.id.clone(), m))
            .collect();

        // Determine the current leaf: a message whose id is not referenced as
        // any other message's parent_id, with the latest created_at timestamp.
        let parent_ids: std::collections::HashSet<&str> = messages
            .values()
            .filter_map(|m| m.parent_id.as_deref())
            .collect();

        let current_leaf_id = messages
            .values()
            .filter(|m| !parent_ids.contains(m.id.as_str()))
            .max_by(|a, b| a.created_at.cmp(&b.created_at))
            .map(|m| m.id.clone());

        Ok(Self {
            session_id: session_id.to_string(),
            current_leaf_id,
            messages,
            project_path: project_path.to_path_buf(),
        })
    }

    /// Append a new message as a child of the current leaf.
    ///
    /// Writes to disk via `SessionStore`, then updates the in-memory map and
    /// advances `current_leaf_id` to the new message.
    pub fn append(&mut self, role: Role, content: String) -> Result<&Message> {
        let msg = Message::new(role, content).with_parent_opt(self.current_leaf_id.clone());

        SessionStore::append_message(&self.project_path, &self.session_id, &msg)
            .with_context(|| format!("failed to persist message to session {}", self.session_id))?;

        let id = msg.id.clone();
        self.current_leaf_id = Some(id.clone());
        self.messages.insert(id.clone(), msg);

        Ok(self.messages.get(&id).expect("just inserted"))
    }

    /// Set `current_leaf_id` to an arbitrary message already in the tree.
    ///
    /// The next `append()` will create a new branch from this point.
    /// Returns `Err` if `from_message_id` is not present in this session.
    pub fn branch(&mut self, from_message_id: &str) -> Result<()> {
        if !self.messages.contains_key(from_message_id) {
            return Err(anyhow::anyhow!(
                "message {} not found in session {}",
                from_message_id,
                self.session_id
            ));
        }
        self.current_leaf_id = Some(from_message_id.to_string());
        Ok(())
    }

    /// Return all messages on the path from the root to `current_leaf_id`,
    /// in root-first order. This is the context slice to send to the LLM.
    pub fn context_messages(&self) -> Vec<&Message> {
        let Some(leaf_id) = &self.current_leaf_id else {
            return vec![];
        };

        let mut chain = vec![];
        let mut current_id = leaf_id.as_str();
        loop {
            let Some(msg) = self.messages.get(current_id) else {
                break;
            };
            chain.push(msg);
            match msg.parent_id.as_deref() {
                Some(parent_id) => current_id = parent_id,
                None => break,
            }
        }

        chain.reverse();
        chain
    }

    /// All messages in the tree (order not guaranteed).
    pub fn all_messages(&self) -> impl Iterator<Item = &Message> {
        self.messages.values()
    }
}

// ---------------------------------------------------------------------------
// Helper extension on Message
// ---------------------------------------------------------------------------

trait WithParentOpt {
    fn with_parent_opt(self, parent_id: Option<String>) -> Self;
}

impl WithParentOpt for Message {
    fn with_parent_opt(self, parent_id: Option<String>) -> Self {
        match parent_id {
            Some(id) => self.with_parent(id),
            None => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn project_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".gantry").join("sessions")).unwrap();
        tmp
    }

    #[test]
    fn create_returns_empty_manager() {
        let tmp = project_dir();
        let mgr = SessionManager::create(tmp.path()).unwrap();
        assert!(mgr.current_leaf_id.is_none());
        assert_eq!(mgr.context_messages().len(), 0);
    }

    #[test]
    fn append_advances_leaf_and_persists() {
        let tmp = project_dir();
        let mut mgr = SessionManager::create(tmp.path()).unwrap();

        mgr.append(Role::User, "hello".to_string()).unwrap();
        assert!(mgr.current_leaf_id.is_some());

        mgr.append(Role::Assistant, "hi".to_string()).unwrap();

        let ctx = mgr.context_messages();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].content, "hello");
        assert_eq!(ctx[1].content, "hi");

        // Verify parent linkage
        let leaf = ctx[1];
        assert_eq!(leaf.parent_id.as_deref(), Some(ctx[0].id.as_str()));
    }

    #[test]
    fn branch_switches_active_leaf() {
        let tmp = project_dir();
        let mut mgr = SessionManager::create(tmp.path()).unwrap();

        let msg1_id = {
            let m = mgr.append(Role::User, "root".to_string()).unwrap();
            m.id.clone()
        };
        mgr.append(Role::Assistant, "branch A".to_string()).unwrap();

        // Branch back to the root message
        mgr.branch(&msg1_id).unwrap();
        mgr.append(Role::User, "branch B".to_string()).unwrap();

        let ctx = mgr.context_messages();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].content, "root");
        assert_eq!(ctx[1].content, "branch B");
    }

    #[test]
    fn branch_errors_on_unknown_id() {
        let tmp = project_dir();
        let mut mgr = SessionManager::create(tmp.path()).unwrap();
        assert!(mgr.branch("nonexistent-id").is_err());
    }

    #[test]
    fn load_restores_session_and_picks_latest_leaf() {
        let tmp = project_dir();
        let session_id = {
            let mut mgr = SessionManager::create(tmp.path()).unwrap();
            mgr.append(Role::User, "first".to_string()).unwrap();
            mgr.append(Role::Assistant, "second".to_string()).unwrap();
            mgr.session_id.clone()
        };

        let mgr = SessionManager::load(tmp.path(), &session_id).unwrap();
        let ctx = mgr.context_messages();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].content, "first");
        assert_eq!(ctx[1].content, "second");
    }
}
