use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::log::SessionLog;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub created_at_ms: u64,
}

/// Manages session files under `{project_path}/.gantry/sessions/`.
pub(crate) struct SessionRegistry {
    sessions_dir: PathBuf,
}

impl SessionRegistry {
    /// Initialise the registry, creating the sessions directory if needed.
    pub fn new(project_path: &Path) -> Result<Self> {
        let sessions_dir = project_path.join(".gantry").join("sessions");
        std::fs::create_dir_all(&sessions_dir).with_context(|| {
            format!(
                "failed to create sessions dir at {}",
                sessions_dir.display()
            )
        })?;
        Ok(Self { sessions_dir })
    }

    /// Get the session log for the given session.
    pub fn session_log(&self, session_id: &str) -> Result<SessionLog> {
        SessionLog::new(&self.sessions_dir, session_id)
    }

    /// List all sessions, sorted by creation time (oldest first).
    pub fn list(&self) -> Result<Vec<SessionInfo>> {
        let mut sessions = vec![];
        for entry in std::fs::read_dir(&self.sessions_dir).with_context(|| {
            format!(
                "failed to read sessions dir {}",
                self.sessions_dir.display()
            )
        })? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some((ms_str, id)) = stem.split_once('_') else {
                continue;
            };
            let Ok(created_at_ms) = ms_str.parse::<u64>() else {
                continue;
            };
            sessions.push(SessionInfo {
                id: id.to_string(),
                created_at_ms,
            });
        }
        sessions.sort_by_key(|s| s.created_at_ms);
        Ok(sessions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use tempfile::TempDir;

    fn registry() -> (TempDir, SessionRegistry) {
        let tmp = TempDir::new().unwrap();
        let r = SessionRegistry::new(tmp.path()).unwrap();
        (tmp, r)
    }

    #[test]
    fn new_creates_sessions_dir() {
        let tmp = TempDir::new().unwrap();
        SessionRegistry::new(tmp.path()).unwrap();
        assert!(tmp.path().join(".gantry").join("sessions").exists());
    }

    #[test]
    fn list_returns_empty_when_no_sessions() {
        let (_tmp, r) = registry();
        assert!(r.list().unwrap().is_empty());
    }

    #[test]
    fn session_log_creates_file_and_is_listed() {
        let (_tmp, r) = registry();
        let id = Uuid::new_v4().to_string();
        r.session_log(&id).unwrap();

        let sessions = r.list().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
    }

    #[test]
    fn session_log_is_idempotent() {
        let (_tmp, r) = registry();
        let id = Uuid::new_v4().to_string();
        r.session_log(&id).unwrap();
        r.session_log(&id).unwrap();

        assert_eq!(r.list().unwrap().len(), 1);
    }

    #[test]
    fn list_sorts_by_creation_time() {
        let (_tmp, r) = registry();
        let id1 = Uuid::new_v4().to_string();
        r.session_log(&id1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let id2 = Uuid::new_v4().to_string();
        r.session_log(&id2).unwrap();

        let sessions = r.list().unwrap();
        assert_eq!(sessions[0].id, id1);
        assert_eq!(sessions[1].id, id2);
    }
}
