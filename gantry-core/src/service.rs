use crate::SessionInfo;
use crate::chat::events::StreamMessageRequest;
use crate::chat::stream::{StreamEvent, stream_message, to_rig_messages};
use crate::chat::{Message, PendingMessage, Role};
use crate::project::ProjectRegistry;
use crate::project::resource_loader::discover_agents_md;
use crate::project::system_prompt::build_system_prompt;
use crate::provider::agent_factory::RigAgentFactory;
use crate::provider::{ModelId, ModelSelection, ProviderId};
use crate::session::Session;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};

pub struct SessionHandle {
    pub project_path: PathBuf,
    pub session_id: String,
    session: Arc<Mutex<Session>>,
    active_selection: Arc<Mutex<ModelSelection>>,
    agent_factory: RigAgentFactory,
}

impl SessionHandle {
    fn new(
        project_path: PathBuf,
        session: Session,
        agent_factory: RigAgentFactory,
        default_selection: ModelSelection,
    ) -> Self {
        let session_id = session.session_id.clone();
        Self {
            project_path,
            session_id,
            session: Arc::new(Mutex::new(session)),
            active_selection: Arc::new(Mutex::new(default_selection)),
            agent_factory,
        }
    }

    pub async fn get_messages(&self) -> Vec<Message> {
        self.session.lock().await.context_messages()
    }

    pub async fn get_tree(&self) -> crate::session::SessionTree {
        self.session.lock().await.session_tree()
    }

    pub async fn branch(&self, entry_id: String) -> Result<()> {
        self.session.lock().await.branch(&entry_id)
    }

    pub async fn get_active_selection(&self) -> ModelSelection {
        self.active_selection.lock().await.clone()
    }

    pub async fn set_active_provider(&self, provider_id: ProviderId) -> Result<()> {
        let model_id = self
            .agent_factory
            .catalog()
            .provider_default_model(&provider_id)?
            .clone();
        self.set_active_selection(ModelSelection {
            provider_id,
            model_id,
        })
        .await
    }

    pub async fn set_active_model(&self, model_id: ModelId) -> Result<()> {
        let provider_id = self.get_active_selection().await.provider_id;
        self.set_active_selection(ModelSelection {
            provider_id,
            model_id,
        })
        .await
    }

    pub async fn set_active_selection(&self, selection: ModelSelection) -> Result<()> {
        self.agent_factory.catalog().selection(&selection)?;
        *self.active_selection.lock().await = selection;
        Ok(())
    }

    pub async fn send_message(&self, content: String) -> Vec<Message> {
        dbg!("session.send_message.request", &content);
        {
            let mut sess = self.session.lock().await;
            sess.append(Role::User, content)
                .unwrap_or_else(|_| panic!("failed to persist message"));
        }

        let context = self.session.lock().await.context_messages();
        let selection = self.get_active_selection().await;
        let system_prompt = build_system_prompt(&discover_agents_md(&self.project_path));
        let mut rig_messages = to_rig_messages(context);
        dbg!("session.send_message.snapshot_len", rig_messages.len());
        let response = match rig_messages.pop() {
            Some(prompt) => match self
                .agent_factory
                .agent(&selection, Some(&system_prompt))
                .await
            {
                Ok(agent) => match agent.chat(prompt, rig_messages).await {
                    Ok(content) => {
                        dbg!("session.send_message.llm_ok_len", content.len());
                        Message::new(Role::Assistant, content)
                    }
                    Err(err) => {
                        dbg!("session.send_message.llm_err", err.to_string());
                        Message::new(Role::Error, err.to_string())
                    }
                },
                Err(err) => {
                    dbg!("session.send_message.agent_err", err.to_string());
                    Message::new(Role::Error, err.to_string())
                }
            },
            None => Message::new(
                Role::Error,
                "cannot generate response with empty message history",
            ),
        };

        {
            let mut sess = self.session.lock().await;
            let _ = sess.append(response.role, response.content.clone());
        }

        let messages = self.session.lock().await.context_messages();
        dbg!("session.send_message.response_messages_len", messages.len());
        messages
    }

    /// Starts streaming a message. Returns the pending message placeholder, a cancel sender,
    /// and a receiver of stream events. The caller is responsible for driving the event receiver
    /// and handling cancellation via the cancel sender.
    pub async fn stream_message(
        &self,
        req: StreamMessageRequest,
    ) -> Result<(
        PendingMessage,
        oneshot::Sender<()>,
        mpsc::Receiver<StreamEvent>,
    )> {
        stream_message(
            req,
            &self.project_path,
            &self.session,
            &self.active_selection,
            &self.agent_factory,
        )
        .await
    }
}

#[derive(Clone)]
pub struct AppService {
    registry: Arc<ProjectRegistry>,
    sessions: Arc<Mutex<HashMap<String, Arc<SessionHandle>>>>,
    agent_factory: RigAgentFactory,
}

impl AppService {
    pub fn new(agent_factory: RigAgentFactory, registry_path: PathBuf) -> Self {
        Self {
            registry: Arc::new(ProjectRegistry::new(registry_path)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            agent_factory,
        }
    }

    pub fn register_project(&self, path: &std::path::Path) -> Result<()> {
        self.registry.register(path)
    }

    pub fn list_projects(&self) -> Result<Vec<PathBuf>> {
        self.registry.list()
    }

    pub fn unregister_project(&self, path: &Path) -> Result<()> {
        self.registry.unregister(path)
    }

    pub fn create_session(&self, project_path: &std::path::Path) -> Result<String> {
        let abs = project_path.canonicalize().map_err(|_| {
            anyhow::anyhow!("project path does not exist: {}", project_path.display())
        })?;
        let projects = self.registry.list()?;
        if !projects.contains(&abs) {
            return Err(anyhow::anyhow!("project not registered: {}", abs.display()));
        }
        let session = Session::create(&abs)?;
        Ok(session.session_id)
    }

    pub fn list_sessions(&self, project_path: &std::path::Path) -> Result<Vec<SessionInfo>> {
        let abs = project_path.canonicalize().map_err(|_| {
            anyhow::anyhow!("project path does not exist: {}", project_path.display())
        })?;
        Session::list(&abs)
    }

    /// Returns an `Arc<SessionHandle>`, creating it in memory if needed.
    /// Returns an error if the session does not exist on disk.
    pub async fn get_or_load_session(
        &self,
        project_path_str: &str,
        session_id: &str,
    ) -> Result<Arc<SessionHandle>> {
        let project_path = std::path::Path::new(project_path_str);
        let abs = project_path
            .canonicalize()
            .map_err(|_| anyhow::anyhow!("project path does not exist: {}", project_path_str))?;

        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(session_id) {
            return Ok(session.clone());
        }

        let session = Session::load(&abs, session_id)?;
        let default_selection = self
            .agent_factory
            .catalog()
            .default_selection()
            .expect("provider catalog must be valid");

        let handle = Arc::new(SessionHandle::new(
            abs,
            session,
            self.agent_factory.clone(),
            default_selection,
        ));
        sessions.insert(session_id.to_string(), handle.clone());
        Ok(handle)
    }

    /// Called when a client disconnects. Removes the session from the in-memory map
    /// if no other clients hold a reference to it (Arc strong count == 1 means only the map holds it).
    pub async fn release_session(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(session_id) {
            if Arc::strong_count(session) <= 2 {
                sessions.remove(session_id);
                dbg!("app.release_session.evicted", session_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::log::LogEntry;
    use crate::session::tree::{Branch, build_branch};
    use tempfile::TempDir;

    fn project_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".gantry").join("sessions")).unwrap();
        tmp
    }

    fn entries_from_session(session: &Session) -> HashMap<String, LogEntry> {
        session
            .all_entries()
            .map(|e| (e.id().to_string(), e.clone()))
            .collect()
    }

    fn build_branch_for_test(session: &Session) -> (Branch, HashMap<String, LogEntry>) {
        let entries = entries_from_session(session);
        let root_id = entries
            .values()
            .find(|e| e.parent_id().is_none())
            .map(|e| e.id().to_string());
        let branch = build_branch(&entries, root_id, 0);
        (branch, entries)
    }

    #[test]
    fn build_branch_empty() {
        let entries = HashMap::new();
        let branch = build_branch(&entries, None, 0);
        assert!(branch.nodes.is_empty());
    }

    #[test]
    fn build_branch_linear() {
        let tmp = project_dir();
        let mut session = Session::create(tmp.path()).unwrap();
        session.append(Role::User, "root".into()).unwrap();
        session.append(Role::Assistant, "mid".into()).unwrap();
        session.append(Role::User, "leaf".into()).unwrap();

        let (branch, _) = build_branch_for_test(&session);

        assert_eq!(branch.depth, 0);
        assert_eq!(branch.nodes.len(), 3);
        assert!(branch.nodes.iter().all(|n| n.branches.is_empty()));
    }

    #[test]
    fn build_branch_two_children() {
        let tmp = project_dir();
        let mut session = Session::create(tmp.path()).unwrap();
        let root_id = session
            .append(Role::User, "root".into())
            .unwrap()
            .base
            .id
            .clone();
        session.append(Role::Assistant, "child A".into()).unwrap();
        session.branch(&root_id).unwrap();
        session.append(Role::Assistant, "child B".into()).unwrap();

        let (branch, _) = build_branch_for_test(&session);

        assert_eq!(branch.depth, 0);
        assert_eq!(branch.nodes.len(), 1);
        assert_eq!(branch.nodes[0].content, "root");
        assert_eq!(branch.nodes[0].branches.len(), 2);
        assert_eq!(branch.nodes[0].branches[0].depth, 1);
        assert_eq!(branch.nodes[0].branches[0].nodes.len(), 1);
        assert_eq!(branch.nodes[0].branches[1].depth, 1);
        assert_eq!(branch.nodes[0].branches[1].nodes.len(), 1);
    }

    #[test]
    fn build_branch_linear_then_fork() {
        let tmp = project_dir();
        let mut session = Session::create(tmp.path()).unwrap();
        session.append(Role::User, "root".into()).unwrap();
        let mid_id = session
            .append(Role::Assistant, "mid".into())
            .unwrap()
            .base
            .id
            .clone();
        session.append(Role::User, "child A".into()).unwrap();
        session.branch(&mid_id).unwrap();
        session.append(Role::User, "child B".into()).unwrap();

        let (branch, _) = build_branch_for_test(&session);

        assert_eq!(branch.depth, 0);
        assert_eq!(branch.nodes.len(), 2);
        assert_eq!(branch.nodes[0].content, "root");
        assert!(branch.nodes[0].branches.is_empty());
        assert_eq!(branch.nodes[1].content, "mid");
        assert_eq!(branch.nodes[1].branches.len(), 2);
        assert_eq!(branch.nodes[1].branches[0].depth, 1);
        assert_eq!(branch.nodes[1].branches[1].depth, 1);
    }

    #[test]
    fn build_branch_deep_nest() {
        let tmp = project_dir();
        let mut session = Session::create(tmp.path()).unwrap();
        let root_id = session
            .append(Role::User, "root".into())
            .unwrap()
            .base
            .id
            .clone();
        session.append(Role::Assistant, "A".into()).unwrap();
        let a_id = session.current_leaf_id.clone().unwrap();
        session.append(Role::User, "B".into()).unwrap();
        session.branch(&a_id).unwrap();
        session.append(Role::User, "C".into()).unwrap();
        session.branch(&root_id).unwrap();
        session.append(Role::Assistant, "D".into()).unwrap();

        let (branch, _) = build_branch_for_test(&session);

        assert_eq!(branch.nodes.len(), 1);
        assert_eq!(branch.nodes[0].branches.len(), 2);

        let sub_with_a = branch.nodes[0]
            .branches
            .iter()
            .find(|b| b.nodes[0].content == "A")
            .unwrap();
        assert_eq!(sub_with_a.depth, 1);
        assert_eq!(sub_with_a.nodes[0].branches.len(), 2);
        assert_eq!(sub_with_a.nodes[0].branches[0].depth, 2);

        let sub_with_d = branch.nodes[0]
            .branches
            .iter()
            .find(|b| b.nodes[0].content == "D")
            .unwrap();
        assert_eq!(sub_with_d.depth, 1);
        assert!(sub_with_d.nodes[0].branches.is_empty());
    }
}
