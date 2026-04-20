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
pub enum LogEntry {
    Message(MessageEntry),
}

impl LogEntry {
    pub fn id(&self) -> &str {
        match self {
            LogEntry::Message(e) => &e.base.id,
        }
    }

    pub fn parent_id(&self) -> Option<&str> {
        match self {
            LogEntry::Message(e) => e.base.parent_id.as_deref(),
        }
    }

    pub fn created_at(&self) -> &str {
        match self {
            LogEntry::Message(e) => &e.base.created_at,
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

/// Handles entry-level I/O for a single session's JSONL file.
pub struct SessionLog {
    path: PathBuf,
}

impl SessionLog {
    /// Open or create the session file for `session_id` under `sessions_dir`.
    pub fn new(sessions_dir: &Path, session_id: &str) -> Result<Self> {
        let suffix = format!("_{}.jsonl", session_id);
        let existing = std::fs::read_dir(sessions_dir)
            .with_context(|| format!("failed to read sessions dir {}", sessions_dir.display()))?
            .find_map(|entry| {
                let path = entry.ok()?.path();
                if path.file_name()?.to_str()?.ends_with(&suffix) {
                    Some(path)
                } else {
                    None
                }
            });

        if let Some(path) = existing {
            return Ok(Self { path });
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system time before UNIX_EPOCH")?
            .as_millis() as u64;
        let file_path = sessions_dir.join(format!("{}_{}.jsonl", now_ms, session_id));

        let header = SessionHeader::new(session_id.to_string());
        let mut file = File::create(&file_path)
            .with_context(|| format!("failed to create session file {}", file_path.display()))?;
        writeln!(
            file,
            "{}",
            serde_json::to_string(&header).context("failed to serialize session header")?
        )
        .with_context(|| format!("failed to write header to {}", file_path.display()))?;

        Ok(Self { path: file_path })
    }

    /// Load all entries of this session.
    pub fn load_entries(&self) -> Result<Vec<LogEntry>> {
        let contents = std::fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read session file {}", self.path.display()))?;
        let mut entries = vec![];
        for line in contents.lines().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let entry: LogEntry = serde_json::from_str(line)
                .with_context(|| format!("invalid JSON line in {}", self.path.display()))?;
            entries.push(entry);
        }
        Ok(entries)
    }

    /// Append a new entry to the session.
    pub fn append_entry(&self, entry: &LogEntry) -> Result<()> {
        let line = serde_json::to_string(entry).context("failed to serialize entry")?;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .with_context(|| format!("failed to open session file {}", self.path.display()))?;
        writeln!(file, "{}", line)
            .with_context(|| format!("failed to write to session file {}", self.path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Role;
    use tempfile::TempDir;

    fn sessions_dir() -> (TempDir, std::path::PathBuf) {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join(".gantry").join("sessions");
        std::fs::create_dir_all(&dir).unwrap();
        (tmp, dir)
    }

    #[test]
    fn append_and_load_entries_roundtrip() {
        let (_tmp, dir) = sessions_dir();
        let id = Uuid::new_v4().to_string();
        let ms = SessionLog::new(&dir, &id).unwrap();

        let e1 = LogEntry::Message(MessageEntry::new(Role::User, "hello".into(), None));
        let e2 = LogEntry::Message(MessageEntry::new(
            Role::Assistant,
            "hi there".into(),
            Some(e1.id().to_string()),
        ));

        ms.append_entry(&e1).unwrap();
        ms.append_entry(&e2).unwrap();

        let loaded = ms.load_entries().unwrap();
        assert_eq!(loaded.len(), 2);

        let LogEntry::Message(ref m1) = loaded[0];
        let LogEntry::Message(ref m2) = loaded[1];
        assert_eq!(m1.content, "hello");
        assert_eq!(m2.content, "hi there");
        assert!(m1.base.parent_id.is_none());
        assert_eq!(m2.base.parent_id.as_deref(), Some(m1.base.id.as_str()));
    }

    #[test]
    fn load_entries_empty_for_new_session() {
        let (_tmp, dir) = sessions_dir();
        let ms = SessionLog::new(&dir, &Uuid::new_v4().to_string()).unwrap();
        assert!(ms.load_entries().unwrap().is_empty());
    }
}
