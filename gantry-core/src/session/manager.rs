use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::session::store::{MessageEntry, SessionEntry, SessionStore};
use crate::types::{Message, Role};

/// In-memory session state for a single conversation tree.
///
/// All entries from the JSONL file are loaded into a HashMap keyed by entry ID.
/// `current_leaf_id` tracks the tip of the active branch; new entries are
/// appended as children of that node.
pub struct SessionManager {
    pub session_id: String,
    /// The ID of the last entry on the currently active branch.
    /// `None` only when the session has no entries yet.
    pub current_leaf_id: Option<String>,
    /// All entries in the tree, keyed by entry ID.
    entries: HashMap<String, SessionEntry>,
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
            entries: HashMap::new(),
            project_path: project_path.to_path_buf(),
        })
    }

    /// Load an existing session from disk into memory.
    ///
    /// `current_leaf_id` is initialised to the most recently created entry
    /// that has no children (i.e. a leaf node with the latest `created_at`).
    pub fn load(project_path: &Path, session_id: &str) -> Result<Self> {
        let entries_vec = SessionStore::load_entries(project_path, session_id)
            .with_context(|| format!("failed to load session {}", session_id))?;

        let entries: HashMap<String, SessionEntry> = entries_vec
            .into_iter()
            .map(|e| (e.id().to_string(), e))
            .collect();

        let parent_ids: std::collections::HashSet<&str> =
            entries.values().filter_map(|e| e.parent_id()).collect();

        let current_leaf_id = entries
            .values()
            .filter(|e| !parent_ids.contains(e.id()))
            .max_by(|a, b| a.created_at().cmp(b.created_at()))
            .map(|e| e.id().to_string());

        Ok(Self {
            session_id: session_id.to_string(),
            current_leaf_id,
            entries,
            project_path: project_path.to_path_buf(),
        })
    }

    /// Append a new message entry as a child of the current leaf.
    ///
    /// Writes to disk via `SessionStore`, updates the in-memory map, and
    /// advances `current_leaf_id` to the new entry.
    pub fn append(&mut self, role: Role, content: String) -> Result<&MessageEntry> {
        let entry = SessionEntry::Message(MessageEntry::new(
            role,
            content,
            self.current_leaf_id.clone(),
        ));

        SessionStore::append_entry(&self.project_path, &self.session_id, &entry)
            .with_context(|| format!("failed to persist entry to session {}", self.session_id))?;

        let id = entry.id().to_string();
        self.current_leaf_id = Some(id.clone());
        self.entries.insert(id.clone(), entry);

        let SessionEntry::Message(ref msg) = self.entries[&id];
        Ok(msg)
    }

    /// Set `current_leaf_id` to an arbitrary entry already in the tree.
    ///
    /// The next `append()` will create a new branch from this point.
    /// Returns `Err` if `from_entry_id` is not present in this session.
    pub fn branch(&mut self, from_entry_id: &str) -> Result<()> {
        if !self.entries.contains_key(from_entry_id) {
            return Err(anyhow::anyhow!(
                "entry {} not found in session {}",
                from_entry_id,
                self.session_id
            ));
        }
        self.current_leaf_id = Some(from_entry_id.to_string());
        Ok(())
    }

    /// Return all messages on the path from the root to `current_leaf_id`,
    /// in root-first order. This is the context slice to send to the LLM.
    pub fn context_messages(&self) -> Vec<Message> {
        let Some(leaf_id) = &self.current_leaf_id else {
            return vec![];
        };

        let mut chain: Vec<&SessionEntry> = vec![];
        let mut current_id = leaf_id.as_str();
        loop {
            let Some(entry) = self.entries.get(current_id) else {
                break;
            };
            chain.push(entry);
            match entry.parent_id() {
                Some(parent_id) => current_id = parent_id,
                None => break,
            }
        }

        chain.reverse();
        chain
            .into_iter()
            .map(|e| match e {
                SessionEntry::Message(m) => m.to_message(),
            })
            .collect()
    }

    /// All entries in the tree (order not guaranteed).
    pub fn all_entries(&self) -> impl Iterator<Item = &SessionEntry> {
        self.entries.values()
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
    }

    #[test]
    fn append_sets_parent_id() {
        let tmp = project_dir();
        let mut mgr = SessionManager::create(tmp.path()).unwrap();

        let first_id = mgr
            .append(Role::User, "root".to_string())
            .unwrap()
            .base
            .id
            .clone();
        mgr.append(Role::Assistant, "reply".to_string()).unwrap();

        // The second entry should reference the first as parent
        let leaf_id = mgr.current_leaf_id.clone().unwrap();
        let SessionEntry::Message(ref leaf) = mgr.entries[&leaf_id];
        assert_eq!(leaf.base.parent_id.as_deref(), Some(first_id.as_str()));
    }

    #[test]
    fn branch_switches_active_leaf() {
        let tmp = project_dir();
        let mut mgr = SessionManager::create(tmp.path()).unwrap();

        let root_id = mgr
            .append(Role::User, "root".to_string())
            .unwrap()
            .base
            .id
            .clone();
        mgr.append(Role::Assistant, "branch A".to_string()).unwrap();

        mgr.branch(&root_id).unwrap();
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
