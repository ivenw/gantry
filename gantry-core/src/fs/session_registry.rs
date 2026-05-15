use anyhow::{Context, Result};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::message::Message;
use crate::metrics::Usage;
use crate::session::registry::{SessionInfo, SessionRegistry};
use crate::session::{ChildNode, NodeId, RootNode, Session, SessionHistory, SessionId, StoredNode};

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

/// A JSONL-serializable mirror of [`StoredNode`] with a `type` tag for format discrimination.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum FsStoredNode {
    Root(FsRootNode),
    Child(FsChildNode),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FsRootNode {
    id: NodeId,
    timestamp: Timestamp,
    message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FsChildNode {
    id: NodeId,
    parent_id: NodeId,
    timestamp: Timestamp,
    message: Message,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    usage: Option<Usage>,
}

impl From<FsStoredNode> for StoredNode {
    fn from(fs: FsStoredNode) -> Self {
        match fs {
            FsStoredNode::Root(r) => StoredNode::Root(RootNode {
                id: r.id,
                timestamp: r.timestamp,
                message: r.message,
            }),
            FsStoredNode::Child(c) => StoredNode::Child(ChildNode {
                id: c.id,
                parent_id: c.parent_id,
                timestamp: c.timestamp,
                message: c.message,
                usage: c.usage,
            }),
        }
    }
}

impl From<&StoredNode> for FsStoredNode {
    fn from(node: &StoredNode) -> Self {
        match node {
            StoredNode::Root(r) => FsStoredNode::Root(FsRootNode {
                id: r.id.clone(),
                timestamp: r.timestamp,
                message: r.message.clone(),
            }),
            StoredNode::Child(c) => FsStoredNode::Child(FsChildNode {
                id: c.id.clone(),
                parent_id: c.parent_id.clone(),
                timestamp: c.timestamp,
                message: c.message.clone(),
                usage: c.usage.clone(),
            }),
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
    /// Appends a stored node as a JSON line to the session file.
    fn append(&self, node: &StoredNode) -> Result<()> {
        let line =
            serde_json::to_string(&FsStoredNode::from(node)).context("failed to serialize node")?;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .with_context(|| format!("failed to open session file {}", self.path.display()))?;
        writeln!(file, "{}", line)
            .with_context(|| format!("failed to write to session file {}", self.path.display()))?;
        Ok(())
    }

    /// Reads all stored nodes from the session file, skipping the header line.
    fn nodes(&self) -> Result<Vec<StoredNode>> {
        let contents = std::fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read session file {}", self.path.display()))?;
        let mut nodes = vec![];
        for line in contents.lines().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let node: FsStoredNode = serde_json::from_str(line)
                .with_context(|| format!("invalid JSON line in {}", self.path.display()))?;
            nodes.push(node.into());
        }
        Ok(nodes)
    }
}

/// Filesystem-backed session registry storing JSONL logs under `{project_config_dir}/sessions/`.
pub struct FsSessionRegistry {
    sessions_dir: PathBuf,
}

impl FsSessionRegistry {
    /// Initialises the registry, creating `sessions_dir` if needed.
    pub fn new(sessions_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(sessions_dir).with_context(|| {
            format!(
                "failed to create sessions dir at {}",
                sessions_dir.display()
            )
        })?;
        Ok(Self {
            sessions_dir: sessions_dir.to_path_buf(),
        })
    }
}

impl SessionRegistry for FsSessionRegistry {
    type History = FsSessionHistory;

    /// Creates a new session with `first_message` as its root node.
    fn create_session(&self, first_message: Message) -> Result<Session<FsSessionHistory>> {
        let session_id = SessionId::new();
        let history = FsSessionHistory::create(&self.sessions_dir, &session_id)?;
        let root = RootNode::new(first_message);
        history.append(&StoredNode::Root(root.clone()))?;
        Ok(Session::new(session_id, root, history))
    }

    /// Loads an existing session from disk by ID, restoring its nodes.
    fn load_session(&self, session_id: &SessionId) -> Result<Session<FsSessionHistory>> {
        let history = FsSessionHistory::open(&self.sessions_dir, session_id)?;
        let id = history.session_id()?;
        Session::restore(id, history)
    }

    /// Lists all sessions that have at least one node, sorted by creation time (oldest first).
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
            // Read only the first node line (line index 1, after the header).
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read session file {}", path.display()))?;
            let first_node_line = contents
                .lines()
                .nth(1)
                .with_context(|| format!("session file has no root node: {}", path.display()))?;
            let FsStoredNode::Root(root) = serde_json::from_str::<FsStoredNode>(first_node_line)
                .with_context(|| format!("invalid root node in session file {}", path.display()))?
            else {
                anyhow::bail!(
                    "expected root node as first entry in session file {}",
                    path.display()
                )
            };
            sessions.push(SessionInfo {
                id,
                timestamp,
                first_message: root.message.text(),
            });
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
    use crate::message::Message;
    use crate::session::{ChildNode, NodeId, RootNode, StoredNode};
    use tempfile::TempDir;

    fn registry() -> (TempDir, FsSessionRegistry) {
        let tmp = TempDir::new().unwrap();
        let r = FsSessionRegistry::new(tmp.path()).unwrap();
        (tmp, r)
    }

    #[test]
    fn new_creates_sessions_dir() {
        let tmp = TempDir::new().unwrap();
        let sessions_dir = tmp.path().join("sessions");
        FsSessionRegistry::new(&sessions_dir).unwrap();
        assert!(sessions_dir.exists());
    }

    #[test]
    fn list_returns_empty_when_no_sessions() {
        let (_tmp, r) = registry();
        assert!(r.list().unwrap().is_empty());
    }

    #[test]
    fn create_session_is_listed() {
        let (_tmp, r) = registry();
        let id = r.create_session(Message::user("hello")).unwrap().session_id;

        let sessions = r.list().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
        assert_eq!(sessions[0].first_message, "hello");
    }

    #[test]
    fn load_session_does_not_duplicate_listing() {
        let (_tmp, r) = registry();
        let id = r.create_session(Message::user("hello")).unwrap().session_id;
        r.load_session(&id).unwrap();

        assert_eq!(r.list().unwrap().len(), 1);
    }

    #[test]
    fn list_sorts_by_creation_time() {
        let (_tmp, r) = registry();
        let id1 = r.create_session(Message::user("a")).unwrap().session_id;
        std::thread::sleep(std::time::Duration::from_millis(2));
        let id2 = r.create_session(Message::user("b")).unwrap().session_id;

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

        let root = RootNode::new(Message::user("hello"));
        let root_id = root.id.clone();
        let child = ChildNode::new(Message::assistant("hi there"), root_id.clone(), None);

        history.append(&StoredNode::Root(root)).unwrap();
        history.append(&StoredNode::Child(child)).unwrap();

        let loaded = history.nodes().unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(matches!(&loaded[0], StoredNode::Root(r) if r.message == Message::user("hello")));
        assert!(
            matches!(&loaded[1], StoredNode::Child(c) if c.message == Message::assistant("hi there") && c.parent_id == root_id)
        );
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

        let root = RootNode::new(Message::user("start"));
        let root_id = root.id.clone();
        let child = ChildNode::new(Message::tool_result("call-abc", "output"), root_id, None);

        history.append(&StoredNode::Root(root)).unwrap();
        history.append(&StoredNode::Child(child)).unwrap();

        let loaded = history.nodes().unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(
            matches!(&loaded[1], StoredNode::Child(c) if c.message == Message::tool_result("call-abc", "output"))
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
