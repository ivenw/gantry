use anyhow::{Context, Result};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::dirs::ProjectConfigDir;
use crate::session::{Node, Session, SessionHistory, SessionId};
use crate::session::registry::{SessionInfo, SessionRegistry};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionHeader {
    #[serde(rename = "type")]
    kind: String,
    session_id: SessionId,
    created_at: Timestamp,
}

impl SessionHeader {
    fn new(id: &SessionId) -> Self {
        Self {
            kind: "header".to_string(),
            session_id: id.clone(),
            created_at: Timestamp::now(),
        }
    }
}

/// Filesystem-backed session history storing nodes as JSONL.
pub struct FsSessionHistory {
    path: PathBuf,
}

impl FsSessionHistory {
    /// Creates a new session file for `session_id` under `sessions_dir`, writing its header.
    fn create(sessions_dir: &Path, session_id: &SessionId) -> Result<Self> {
        let path = sessions_dir.join(session_filename(session_id)?);
        let header = SessionHeader::new(session_id);
        let mut file = File::create(&path)
            .with_context(|| format!("failed to create session file {}", path.display()))?;
        writeln!(
            file,
            "{}",
            serde_json::to_string(&header).context("failed to serialize session header")?
        )
        .with_context(|| format!("failed to write header to {}", path.display()))?;
        Ok(Self { path })
    }

    /// Opens an existing session file for `session_id` under `sessions_dir`.
    ///
    /// Returns an error if no matching file is found.
    fn open(sessions_dir: &Path, session_id: &SessionId) -> Result<Self> {
        let suffix = format!("_{}.jsonl", session_id);
        let path = std::fs::read_dir(sessions_dir)
            .with_context(|| format!("failed to read sessions dir {}", sessions_dir.display()))?
            .find_map(|entry| {
                let path = entry.ok()?.path();
                if path.file_name()?.to_str()?.ends_with(&suffix) {
                    Some(path)
                } else {
                    None
                }
            })
            .with_context(|| format!("session file not found for {}", session_id))?;
        Ok(Self { path })
    }

    /// Reads the session ID stored in the header line of this file.
    fn session_id(&self) -> Result<SessionId> {
        let contents = std::fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read session file {}", self.path.display()))?;
        let first = contents
            .lines()
            .next()
            .with_context(|| format!("session file is empty: {}", self.path.display()))?;
        let header: SessionHeader = serde_json::from_str(first)
            .with_context(|| format!("invalid header in {}", self.path.display()))?;
        Ok(header.session_id)
    }
}

impl SessionHistory for FsSessionHistory {
    /// Appends a new node as a JSON line to the session file.
    fn append(&self, node: &Node) -> Result<()> {
        let line = serde_json::to_string(node).context("failed to serialize node")?;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .with_context(|| format!("failed to open session file {}", self.path.display()))?;
        writeln!(file, "{}", line)
            .with_context(|| format!("failed to write to session file {}", self.path.display()))?;
        Ok(())
    }

    /// Reads all nodes from the session file, skipping the header line.
    fn nodes(&self) -> Result<Vec<Node>> {
        let contents = std::fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read session file {}", self.path.display()))?;
        let mut nodes = vec![];
        for line in contents.lines().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let node: Node = serde_json::from_str(line)
                .with_context(|| format!("invalid JSON line in {}", self.path.display()))?;
            nodes.push(node);
        }
        Ok(nodes)
    }
}

/// Filesystem-backed session registry storing JSONL logs under `{project_config_dir}/sessions/`.
pub struct FsSessionRegistry {
    sessions_dir: PathBuf,
}

impl FsSessionRegistry {
    /// Initialises the registry, creating the sessions directory if needed.
    pub fn new(project_config_dir: &ProjectConfigDir) -> Result<Self> {
        let sessions_dir = project_config_dir.path().join("sessions");
        std::fs::create_dir_all(&sessions_dir).with_context(|| {
            format!(
                "failed to create sessions dir at {}",
                sessions_dir.display()
            )
        })?;
        Ok(Self { sessions_dir })
    }
}

impl SessionRegistry for FsSessionRegistry {
    type History = FsSessionHistory;

    /// Creates a new empty session, assigning it a fresh ID and persisting its log file.
    fn create_session(&self) -> Result<Session<FsSessionHistory>> {
        let session_id = SessionId::new();
        let history = FsSessionHistory::create(&self.sessions_dir, &session_id)?;
        Ok(Session::new(session_id, history))
    }

    /// Loads an existing session from disk by ID, restoring its nodes.
    fn load_session(&self, session_id: &SessionId) -> Result<Session<FsSessionHistory>> {
        let history = FsSessionHistory::open(&self.sessions_dir, session_id)?;
        let id = history.session_id()?;
        Session::restore(id, history)
    }

    /// Lists all sessions, sorted by creation time (oldest first).
    fn list(&self) -> Result<Vec<SessionInfo>> {
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
            let Some((ms_str, id_str)) = stem.split_once('_') else {
                continue;
            };
            let Ok(ms) = ms_str.parse::<i64>() else {
                continue;
            };
            let Ok(timestamp) = Timestamp::from_millisecond(ms) else {
                continue;
            };
            let Ok(id) = id_str.parse::<SessionId>() else {
                continue;
            };
            sessions.push(SessionInfo { id, timestamp });
        }
        sessions.sort_by_key(|s| s.timestamp);
        Ok(sessions)
    }
}

/// Returns a session filename of the form `{ms_since_epoch}_{session_id}.jsonl`.
fn session_filename(session_id: &SessionId) -> Result<String> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time before UNIX_EPOCH")?
        .as_millis() as i64;
    Ok(format!("{}_{}.jsonl", now_ms, session_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirs::ProjectRootDir;
    use crate::session::NodeId;
    use rig::message::Message;
    use tempfile::TempDir;

    fn registry() -> (TempDir, FsSessionRegistry) {
        let tmp = TempDir::new().unwrap();
        let root = ProjectRootDir::new(tmp.path()).unwrap();
        let config_dir = ProjectConfigDir::new(&root).unwrap();
        let r = FsSessionRegistry::new(&config_dir).unwrap();
        (tmp, r)
    }

    #[test]
    fn new_creates_sessions_dir() {
        let tmp = TempDir::new().unwrap();
        let root = ProjectRootDir::new(tmp.path()).unwrap();
        let config_dir = ProjectConfigDir::new(&root).unwrap();
        FsSessionRegistry::new(&config_dir).unwrap();
        assert!(tmp.path().join(".gantry").join("sessions").exists());
    }

    #[test]
    fn list_returns_empty_when_no_sessions() {
        let (_tmp, r) = registry();
        assert!(r.list().unwrap().is_empty());
    }

    #[test]
    fn create_session_is_listed() {
        let (_tmp, r) = registry();
        let id = r.create_session().unwrap().session_id;

        let sessions = r.list().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
    }

    #[test]
    fn load_session_does_not_duplicate_listing() {
        let (_tmp, r) = registry();
        let id = r.create_session().unwrap().session_id;
        r.load_session(&id).unwrap();

        assert_eq!(r.list().unwrap().len(), 1);
    }

    #[test]
    fn list_sorts_by_creation_time() {
        let (_tmp, r) = registry();
        let id1 = r.create_session().unwrap().session_id;
        std::thread::sleep(std::time::Duration::from_millis(2));
        let id2 = r.create_session().unwrap().session_id;

        let sessions = r.list().unwrap();
        assert_eq!(sessions[0].id, id1);
        assert_eq!(sessions[1].id, id2);
    }

    #[test]
    fn append_and_load_nodes_roundtrip() {
        let (_tmp, dir_path) = {
            let tmp = TempDir::new().unwrap();
            let dir = tmp.path().join(".gantry").join("sessions");
            std::fs::create_dir_all(&dir).unwrap();
            (tmp, dir)
        };

        let id = SessionId::new();
        let history = FsSessionHistory::create(&dir_path, &id).unwrap();

        let n1 = Node::new(Message::user("hello"), None);
        let n2 = Node::new(Message::assistant("hi there"), Some(n1.id.clone()));

        history.append(&n1).unwrap();
        history.append(&n2).unwrap();

        let loaded = history.nodes().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].message, Message::user("hello"));
        assert_eq!(loaded[1].message, Message::assistant("hi there"));
        assert!(loaded[0].parent_id.is_none());
        assert_eq!(loaded[1].parent_id.as_ref(), Some(&loaded[0].id));
    }

    #[test]
    fn load_nodes_empty_for_new_session() {
        let (_tmp, dir_path) = {
            let tmp = TempDir::new().unwrap();
            let dir = tmp.path().join(".gantry").join("sessions");
            std::fs::create_dir_all(&dir).unwrap();
            (tmp, dir)
        };

        let history = FsSessionHistory::create(&dir_path, &SessionId::new()).unwrap();
        assert!(history.nodes().unwrap().is_empty());
    }

    #[test]
    fn tool_result_node_roundtrip() {
        let (_tmp, dir_path) = {
            let tmp = TempDir::new().unwrap();
            let dir = tmp.path().join(".gantry").join("sessions");
            std::fs::create_dir_all(&dir).unwrap();
            (tmp, dir)
        };

        let id = SessionId::new();
        let history = FsSessionHistory::create(&dir_path, &id).unwrap();

        let node = Node::new(Message::tool_result("call-abc", "output"), None);
        history.append(&node).unwrap();

        let loaded = history.nodes().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(
            loaded[0].message,
            Message::tool_result("call-abc", "output")
        );
    }

    #[test]
    fn node_id_parse_roundtrip() {
        let id = NodeId::new();
        let s = id.to_string();
        let parsed: NodeId = s.parse().unwrap();
        assert_eq!(id, parsed);
    }
}
