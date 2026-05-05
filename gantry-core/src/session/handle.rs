use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use rig::message::Message;
use tokio::sync::Mutex;

use crate::dirs::{ProjectConfigDir, ProjectRootDir};
use crate::fs::FsSessionRegistry;
use crate::provider::ModelSelection;
use crate::session::registry::SessionRegistry;
use crate::session::{NodeId, Session, SessionId, SessionTree};

type FsSession = Session<crate::fs::session_registry::FsSessionHistory>;

/// Async wrapper around a [`Session`] providing interior mutability and a bound model selection.
pub struct SessionHandle {
    pub project_path: PathBuf,
    inner: Mutex<FsSession>,
    selection: Mutex<ModelSelection>,
}

impl SessionHandle {
    fn new(project_path: PathBuf, session: FsSession, selection: ModelSelection) -> Self {
        Self {
            project_path,
            inner: Mutex::new(session),
            selection: Mutex::new(selection),
        }
    }

    /// Appends a message to the session, persisting it to disk.
    pub async fn append_message(&self, msg: Message) -> Result<()> {
        self.inner.lock().await.append_message(msg)
    }

    /// Returns the current context messages and active model selection.
    pub async fn snapshot(&self) -> (Vec<Message>, ModelSelection) {
        let messages = self.inner.lock().await.context_messages();
        let selection = self.selection.lock().await.clone();
        (messages, selection)
    }

    /// Returns the current context messages.
    pub async fn get_messages(&self) -> Vec<Message> {
        self.inner.lock().await.context_messages()
    }

    /// Returns the active model selection.
    pub async fn get_active_selection(&self) -> ModelSelection {
        self.selection.lock().await.clone()
    }

    /// Replaces the active model selection.
    pub async fn set_active_selection(&self, selection: ModelSelection) {
        *self.selection.lock().await = selection;
    }

    /// Builds and returns the session tree, or `None` if the session has no nodes.
    pub async fn get_tree(&self) -> Option<SessionTree> {
        self.inner.lock().await.as_tree()
    }

    /// Switches the active leaf to the node identified by `node_id_str`.
    pub async fn branch(&self, node_id_str: String) -> Result<()> {
        let node_id: NodeId = node_id_str.parse()?;
        self.inner.lock().await.branch(&node_id)
    }
}

/// In-memory cache of loaded [`SessionHandle`]s, keyed by [`SessionId`].
pub struct SessionManager {
    handles: Mutex<HashMap<SessionId, Arc<SessionHandle>>>,
}

impl SessionManager {
    /// Creates an empty session manager.
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(HashMap::new()),
        }
    }

    /// Creates a new session for the project at `project_path` and caches the handle.
    pub async fn create_session(
        &self,
        project_path: &Path,
        selection: ModelSelection,
    ) -> Result<SessionId> {
        let root = ProjectRootDir::new(project_path)?;
        let config_dir = ProjectConfigDir::new(&root)?;
        let registry = FsSessionRegistry::new(&config_dir)?;
        let session = registry.create_session()?;
        let session_id = session.session_id.clone();
        let handle = Arc::new(SessionHandle::new(
            project_path.to_path_buf(),
            session,
            selection,
        ));
        self.handles.lock().await.insert(session_id.clone(), handle);
        Ok(session_id)
    }

    /// Returns a cached handle or loads the session from disk.
    pub async fn get_or_load(
        &self,
        project_path: &Path,
        session_id: &SessionId,
        selection: ModelSelection,
    ) -> Result<Arc<SessionHandle>> {
        let mut handles = self.handles.lock().await;
        if let Some(handle) = handles.get(session_id) {
            return Ok(handle.clone());
        }
        let root = ProjectRootDir::new(project_path)?;
        let config_dir = ProjectConfigDir::new(&root)?;
        let registry = FsSessionRegistry::new(&config_dir)?;
        let session = registry.load_session(session_id)?;
        let handle = Arc::new(SessionHandle::new(
            project_path.to_path_buf(),
            session,
            selection,
        ));
        handles.insert(session_id.clone(), handle.clone());
        Ok(handle)
    }
}
