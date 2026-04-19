use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::chat::{Message, Role};

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionHeader {
    #[serde(rename = "type")]
    kind: String,
    id: String,
    created_at: String,
}

impl SessionHeader {
    fn new(id: String) -> Self {
        Self {
            kind: "header".to_string(),
            id,
            created_at: now_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SessionEntry {
    Message(MessageEntry),
}

impl SessionEntry {
    pub fn id(&self) -> &str {
        match self {
            SessionEntry::Message(e) => &e.base.id,
        }
    }

    pub fn parent_id(&self) -> Option<&str> {
        match self {
            SessionEntry::Message(e) => e.base.parent_id.as_deref(),
        }
    }

    pub fn created_at(&self) -> &str {
        match self {
            SessionEntry::Message(e) => &e.base.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryBase {
    pub id: String,
    pub parent_id: Option<String>,
    pub created_at: String,
}

impl EntryBase {
    pub fn new(parent_id: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            parent_id,
            created_at: now_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageEntry {
    #[serde(flatten)]
    pub base: EntryBase,
    pub role: Role,
    pub content: String,
}

impl MessageEntry {
    pub fn new(role: Role, content: String, parent_id: Option<String>) -> Self {
        Self {
            base: EntryBase::new(parent_id),
            role,
            content,
        }
    }

    pub fn to_message(&self) -> Message {
        Message::new(self.role, &self.content)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub created_at_ms: u64,
}

pub struct SessionStore;

impl SessionStore {
    /// Create a new session under `project_path/.gantry/sessions/`.
    /// Returns the new session UUID.
    pub fn create(project_path: &Path) -> Result<String> {
        let sessions_dir = Self::sessions_dir(project_path);
        std::fs::create_dir_all(&sessions_dir).with_context(|| {
            format!(
                "failed to create sessions dir at {}",
                sessions_dir.display()
            )
        })?;

        let id = Uuid::new_v4().to_string();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system time before UNIX_EPOCH")?
            .as_millis() as u64;
        let filename = format!("{}_{}.jsonl", now_ms, id);
        let file_path = sessions_dir.join(&filename);

        let header = SessionHeader::new(id.clone());
        let mut file = File::create(&file_path)
            .with_context(|| format!("failed to create session file {}", file_path.display()))?;
        writeln!(
            file,
            "{}",
            serde_json::to_string(&header).context("failed to serialize session header")?
        )
        .with_context(|| format!("failed to write header to {}", file_path.display()))?;

        Ok(id)
    }

    /// List all sessions for `project_path`, sorted by creation time (oldest first).
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

    /// Check whether a session file exists under `project_path`.
    pub fn exists(project_path: &Path, session_id: &str) -> bool {
        Self::find_session_path(project_path, session_id).is_some()
    }

    /// Append an entry to a session's `.jsonl` file.
    pub fn append_entry(project_path: &Path, session_id: &str, entry: &SessionEntry) -> Result<()> {
        let path = Self::find_session_path(project_path, session_id)
            .with_context(|| format!("session not found: {}", session_id))?;
        let line = serde_json::to_string(entry).context("failed to serialize entry")?;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open session file {}", path.display()))?;
        writeln!(file, "{}", line)
            .with_context(|| format!("failed to write to session file {}", path.display()))?;
        Ok(())
    }

    /// Load all entries from a session's `.jsonl` file (skips the header line).
    pub fn load_entries(project_path: &Path, session_id: &str) -> Result<Vec<SessionEntry>> {
        let path = Self::find_session_path(project_path, session_id)
            .with_context(|| format!("session not found: {}", session_id))?;
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read session file {}", path.display()))?;
        let mut entries = vec![];
        for line in contents.lines().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let entry: SessionEntry = serde_json::from_str(line)
                .with_context(|| format!("invalid JSON line in {}", path.display()))?;
            entries.push(entry);
        }
        Ok(entries)
    }

    /// Load the session header (first line of the file).
    #[cfg(test)]
    fn load_header(project_path: &Path, session_id: &str) -> Result<SessionHeader> {
        let path = Self::find_session_path(project_path, session_id)
            .with_context(|| format!("session not found: {}", session_id))?;
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read session file {}", path.display()))?;
        let first_line = contents
            .lines()
            .next()
            .with_context(|| format!("session file is empty: {}", path.display()))?;
        serde_json::from_str(first_line)
            .with_context(|| format!("invalid header in {}", path.display()))
    }

    fn sessions_dir(project_path: &Path) -> PathBuf {
        project_path.join(".gantry").join("sessions")
    }

    /// Find the path of a session file by scanning for `*_{session_id}.jsonl`.
    fn find_session_path(project_path: &Path, session_id: &str) -> Option<PathBuf> {
        let sessions_dir = Self::sessions_dir(project_path);
        let suffix = format!("_{}.jsonl", session_id);
        std::fs::read_dir(&sessions_dir).ok()?.find_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?;
            if name.ends_with(&suffix) {
                Some(path)
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Role;
    use tempfile::TempDir;

    fn project_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
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
        assert!(SessionStore::exists(tmp.path(), &id));
    }

    #[test]
    fn create_writes_header_as_first_line() {
        let tmp = project_dir();
        let id = SessionStore::create(tmp.path()).unwrap();
        let header = SessionStore::load_header(tmp.path(), &id).unwrap();
        assert_eq!(header.id, id);
        assert_eq!(header.kind, "header");
        assert!(!header.created_at.is_empty());
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
    fn list_sorts_by_creation_time() {
        let tmp = project_dir();
        let id1 = SessionStore::create(tmp.path()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let id2 = SessionStore::create(tmp.path()).unwrap();

        let sessions = SessionStore::list(tmp.path()).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, id1);
        assert_eq!(sessions[1].id, id2);
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
    fn append_and_load_entries_roundtrip() {
        let tmp = project_dir();
        let id = SessionStore::create(tmp.path()).unwrap();

        let e1 = SessionEntry::Message(MessageEntry::new(Role::User, "hello".into(), None));
        let e2 = SessionEntry::Message(MessageEntry::new(
            Role::Assistant,
            "hi there".into(),
            Some(e1.id().to_string()),
        ));

        SessionStore::append_entry(tmp.path(), &id, &e1).unwrap();
        SessionStore::append_entry(tmp.path(), &id, &e2).unwrap();

        let loaded = SessionStore::load_entries(tmp.path(), &id).unwrap();
        assert_eq!(loaded.len(), 2);

        let SessionEntry::Message(ref m1) = loaded[0];
        let SessionEntry::Message(ref m2) = loaded[1];
        assert_eq!(m1.content, "hello");
        assert_eq!(m2.content, "hi there");
        assert!(!m1.base.id.is_empty());
        assert!(!m1.base.created_at.is_empty());
        assert!(m1.base.parent_id.is_none());
        assert_eq!(m2.base.parent_id.as_deref(), Some(m1.base.id.as_str()));
    }

    #[test]
    fn load_entries_returns_empty_for_new_session() {
        let tmp = project_dir();
        let id = SessionStore::create(tmp.path()).unwrap();

        let entries = SessionStore::load_entries(tmp.path(), &id).unwrap();
        assert!(entries.is_empty());
    }
}
