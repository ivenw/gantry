pub mod log;
pub mod registry;
pub mod tree;

pub use registry::SessionInfo;
pub use tree::{Branch, BranchNode, SessionTree};

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

use crate::chat::{Message, Role};
use crate::session::log::{LogEntry, MessageEntry, SessionLog};
use crate::session::registry::SessionRegistry;
use crate::session::tree::build_branch;

pub struct Session {
    pub session_id: String,
    pub current_leaf_id: Option<String>,
    entries: HashMap<String, LogEntry>,
    session_log: SessionLog,
}

impl Session {
    pub fn create(project_path: &Path) -> Result<Self> {
        let session_id = Uuid::new_v4().to_string();
        let session_log = SessionRegistry::new(project_path)?.session_log(&session_id)?;
        Ok(Self {
            session_id,
            current_leaf_id: None,
            entries: HashMap::new(),
            session_log,
        })
    }

    pub fn load(project_path: &Path, session_id: &str) -> Result<Self> {
        let message_store = SessionRegistry::new(project_path)?.session_log(session_id)?;
        let entries_vec = message_store
            .load_entries()
            .with_context(|| format!("failed to load session {}", session_id))?;

        let entries: HashMap<String, LogEntry> = entries_vec
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
            session_log: message_store,
        })
    }

    pub fn list(project_path: &Path) -> Result<Vec<SessionInfo>> {
        SessionRegistry::new(project_path)?.list()
    }

    pub fn append(&mut self, role: Role, content: String) -> Result<&MessageEntry> {
        let entry = LogEntry::Message(MessageEntry::new(
            role,
            content,
            self.current_leaf_id.clone(),
        ));

        self.session_log
            .append_entry(&entry)
            .with_context(|| format!("failed to persist entry to session {}", self.session_id))?;

        let id = entry.id().to_string();
        self.current_leaf_id = Some(id.clone());
        self.entries.insert(id.clone(), entry);

        let LogEntry::Message(ref msg) = self.entries[&id];
        Ok(msg)
    }

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

    pub fn context_messages(&self) -> Vec<Message> {
        let Some(leaf_id) = &self.current_leaf_id else {
            return vec![];
        };

        let mut chain: Vec<&LogEntry> = vec![];
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
                LogEntry::Message(m) => m.to_message(),
            })
            .collect()
    }

    pub fn all_entries(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.values()
    }

    pub fn session_tree(&self) -> SessionTree {
        let entries: HashMap<String, LogEntry> = self
            .all_entries()
            .map(|e| (e.id().to_string(), e.clone()))
            .collect();
        let root_id = entries
            .values()
            .find(|e| e.parent_id().is_none())
            .map(|e| e.id().to_string());
        SessionTree {
            current_leaf_id: self.current_leaf_id.clone(),
            stem: build_branch(&entries, root_id, 0),
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
    fn create_returns_empty_session() {
        let tmp = project_dir();
        let session = Session::create(tmp.path()).unwrap();
        assert!(session.current_leaf_id.is_none());
        assert_eq!(session.context_messages().len(), 0);
    }

    #[test]
    fn append_advances_leaf_and_persists() {
        let tmp = project_dir();
        let mut session = Session::create(tmp.path()).unwrap();

        session.append(Role::User, "hello".to_string()).unwrap();
        assert!(session.current_leaf_id.is_some());

        session.append(Role::Assistant, "hi".to_string()).unwrap();

        let ctx = session.context_messages();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].content, "hello");
        assert_eq!(ctx[1].content, "hi");
    }

    #[test]
    fn append_sets_parent_id() {
        let tmp = project_dir();
        let mut session = Session::create(tmp.path()).unwrap();

        let first_id = session
            .append(Role::User, "root".to_string())
            .unwrap()
            .base
            .id
            .clone();
        session
            .append(Role::Assistant, "reply".to_string())
            .unwrap();

        let leaf_id = session.current_leaf_id.clone().unwrap();
        let LogEntry::Message(ref leaf) = session.entries[&leaf_id];
        assert_eq!(leaf.base.parent_id.as_deref(), Some(first_id.as_str()));
    }

    #[test]
    fn branch_switches_active_leaf() {
        let tmp = project_dir();
        let mut session = Session::create(tmp.path()).unwrap();

        let root_id = session
            .append(Role::User, "root".to_string())
            .unwrap()
            .base
            .id
            .clone();
        session
            .append(Role::Assistant, "branch A".to_string())
            .unwrap();

        session.branch(&root_id).unwrap();
        session
            .append(Role::User, "branch B".to_string())
            .unwrap();

        let ctx = session.context_messages();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].content, "root");
        assert_eq!(ctx[1].content, "branch B");
    }

    #[test]
    fn branch_errors_on_unknown_id() {
        let tmp = project_dir();
        let mut session = Session::create(tmp.path()).unwrap();
        assert!(session.branch("nonexistent-id").is_err());
    }

    #[test]
    fn load_restores_session_and_picks_latest_leaf() {
        let tmp = project_dir();
        let session_id = {
            let mut session = Session::create(tmp.path()).unwrap();
            session.append(Role::User, "first".to_string()).unwrap();
            session
                .append(Role::Assistant, "second".to_string())
                .unwrap();
            session.session_id.clone()
        };

        let session = Session::load(tmp.path(), &session_id).unwrap();
        let ctx = session.context_messages();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].content, "first");
        assert_eq!(ctx[1].content, "second");
    }

    #[test]
    fn list_returns_created_sessions() {
        let tmp = project_dir();
        let id = Session::create(tmp.path()).unwrap().session_id;
        let sessions = Session::list(tmp.path()).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
    }
}
