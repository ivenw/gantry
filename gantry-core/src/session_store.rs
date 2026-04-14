use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionInfo {
    pub id: String,
}

pub struct SessionStore;

impl SessionStore {
    /// Create a new session under `project_path/.gantry/sessions/`.
    /// Returns the new session UUID.
    pub fn create(project_path: &Path) -> Result<String> {
        let sessions_dir = Self::sessions_dir(project_path);
        std::fs::create_dir_all(&sessions_dir)
            .with_context(|| format!("failed to create sessions dir at {}", sessions_dir.display()))?;

        let id = Uuid::new_v4().to_string();
        let file = sessions_dir.join(format!("{}.jsonl", id));
        std::fs::File::create(&file)
            .with_context(|| format!("failed to create session file {}", file.display()))?;

        Ok(id)
    }

    /// List all sessions for `project_path`.
    pub fn list(project_path: &Path) -> Result<Vec<SessionInfo>> {
        let sessions_dir = Self::sessions_dir(project_path);
        if !sessions_dir.exists() {
            return Ok(vec![]);
        }

        let mut sessions = vec![];
        for entry in std::fs::read_dir(&sessions_dir)
            .with_context(|| format!("failed to read sessions dir {}", sessions_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    sessions.push(SessionInfo {
                        id: stem.to_string(),
                    });
                }
            }
        }

        Ok(sessions)
    }

    /// Check whether a session file exists under `project_path`.
    pub fn exists(project_path: &Path, session_id: &str) -> bool {
        Self::session_path(project_path, session_id).exists()
    }

    /// Append a message to a session's `.jsonl` file.
    pub fn append_message(
        project_path: &Path,
        session_id: &str,
        message: &crate::Message,
    ) -> Result<()> {
        let path = Self::session_path(project_path, session_id);
        let line = serde_json::to_string(message)
            .context("failed to serialize message")?;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open session file {}", path.display()))?;
        use std::io::Write;
        writeln!(file, "{}", line)
            .with_context(|| format!("failed to write to session file {}", path.display()))?;
        Ok(())
    }

    /// Load all messages from a session's `.jsonl` file.
    pub fn load_messages(project_path: &Path, session_id: &str) -> Result<Vec<crate::Message>> {
        let path = Self::session_path(project_path, session_id);
        if !path.exists() {
            return Ok(vec![]);
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read session file {}", path.display()))?;
        let mut messages = vec![];
        for line in contents.lines() {
            let msg: crate::Message =
                serde_json::from_str(line).with_context(|| format!("invalid JSON line in {}", path.display()))?;
            messages.push(msg);
        }
        Ok(messages)
    }

    fn sessions_dir(project_path: &Path) -> PathBuf {
        project_path.join(".gantry").join("sessions")
    }

    fn session_path(project_path: &Path, session_id: &str) -> PathBuf {
        Self::sessions_dir(project_path).join(format!("{}.jsonl", session_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, Role};
    use tempfile::TempDir;

    fn project_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        // Pre-create the sessions dir as register_project would
        std::fs::create_dir_all(tmp.path().join(".gantry").join("sessions")).unwrap();
        tmp
    }

    #[test]
    fn list_returns_empty_when_no_sessions_dir() {
        let tmp = TempDir::new().unwrap();
        let sessions = SessionStore::list(tmp.path()).unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn create_returns_uuid_and_creates_file() {
        let tmp = project_dir();
        let id = SessionStore::create(tmp.path()).unwrap();

        assert!(!id.is_empty());
        assert!(tmp
            .path()
            .join(".gantry")
            .join("sessions")
            .join(format!("{}.jsonl", id))
            .exists());
    }

    #[test]
    fn list_returns_created_session() {
        let tmp = project_dir();
        let id = SessionStore::create(tmp.path()).unwrap();

        let sessions = SessionStore::list(tmp.path()).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
    }

    #[test]
    fn create_multiple_sessions() {
        let tmp = project_dir();
        SessionStore::create(tmp.path()).unwrap();
        SessionStore::create(tmp.path()).unwrap();

        let sessions = SessionStore::list(tmp.path()).unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn exists_returns_true_for_created_session() {
        let tmp = project_dir();
        let id = SessionStore::create(tmp.path()).unwrap();
        assert!(SessionStore::exists(tmp.path(), &id));
    }

    #[test]
    fn exists_returns_false_for_missing_session() {
        let tmp = project_dir();
        assert!(!SessionStore::exists(tmp.path(), "nonexistent-uuid"));
    }

    #[test]
    fn append_and_load_messages_roundtrip() {
        let tmp = project_dir();
        let id = SessionStore::create(tmp.path()).unwrap();

        let msg1 = Message::new(Role::User, "hello");
        let msg2 = Message::new(Role::Assistant, "hi there");

        SessionStore::append_message(tmp.path(), &id, &msg1).unwrap();
        SessionStore::append_message(tmp.path(), &id, &msg2).unwrap();

        let loaded = SessionStore::load_messages(tmp.path(), &id).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].content, "hello");
        assert_eq!(loaded[1].content, "hi there");
    }

    #[test]
    fn load_messages_returns_empty_for_new_session() {
        let tmp = project_dir();
        let id = SessionStore::create(tmp.path()).unwrap();

        let messages = SessionStore::load_messages(tmp.path(), &id).unwrap();
        assert!(messages.is_empty());
    }
}
