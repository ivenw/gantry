use crate::SessionInfo;
use crate::chat::events::{AppEvent, InitEvent, StreamMessageRequest};
use crate::chat::stream::{interrupt_stream, stream_message, to_rig_messages};
use crate::chat::{Message, PendingMessage, Role};
use crate::event_bus::EventBus;
use crate::project::ProjectRegistry;
use crate::project::resource_loader::discover_agents_md;
use crate::project::system_prompt::build_system_prompt;
use crate::provider::agent_factory::RigAgentFactory;
use crate::provider::{ModelId, ModelSelection, ProviderId};
use crate::session::registry::SessionRegistry;
use crate::session::manager::SessionManager;
use crate::session::state::ConversationState;
use crate::session::tree::{SessionTree, build_branch};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, broadcast, oneshot};
use uuid::Uuid;

pub struct ActiveSession {
    pub project_path: PathBuf,
    pub session_id: String,
    session_manager: Arc<Mutex<SessionManager>>,
    state: Arc<Mutex<ConversationState>>,
    event_bus: EventBus,
    agent_factory: RigAgentFactory,
    is_streaming: Arc<AtomicBool>,
    cancel_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl ActiveSession {
    fn new(
        project_path: PathBuf,
        session_manager: SessionManager,
        initial_messages: Vec<Message>,
        agent_factory: RigAgentFactory,
        default_selection: ModelSelection,
    ) -> Self {
        let session_id = session_manager.session_id.clone();
        let mut state = ConversationState::new(default_selection);
        state.messages = initial_messages;
        Self {
            project_path,
            session_id,
            session_manager: Arc::new(Mutex::new(session_manager)),
            state: Arc::new(Mutex::new(state)),
            event_bus: EventBus::new(1000),
            agent_factory,
            is_streaming: Arc::new(AtomicBool::new(false)),
            cancel_tx: Arc::new(Mutex::new(None)),
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<AppEvent> {
        self.event_bus.subscribe()
    }

    pub fn is_streaming(&self) -> bool {
        self.is_streaming.load(Ordering::SeqCst)
    }

    pub async fn init_event(&self) -> AppEvent {
        let state = self.state.lock().await;
        AppEvent::Init(InitEvent {
            client_id: Uuid::new_v4().to_string(),
            messages: state.messages.clone(),
            pending_message: state.pending_message.clone(),
        })
    }

    pub async fn get_messages(&self) -> Vec<Message> {
        self.state.lock().await.messages.clone()
    }

    pub async fn clear_messages(&self) {
        self.state.lock().await.messages.clear();
    }

    pub async fn get_tree(&self) -> SessionTree {
        let mgr = self.session_manager.lock().await;
        let current_leaf_id = mgr.current_leaf_id.clone();
        let entries: HashMap<String, crate::session::log::LogEntry> = mgr
            .all_entries()
            .map(|e| (e.id().to_string(), e.clone()))
            .collect();
        let root_id = entries
            .values()
            .find(|e| e.parent_id().is_none())
            .map(|e| e.id().to_string());
        SessionTree {
            current_leaf_id,
            stem: build_branch(&entries, root_id, 0),
        }
    }

    pub async fn branch(&self, entry_id: String) -> Result<()> {
        let messages = {
            let mut mgr = self.session_manager.lock().await;
            mgr.branch(&entry_id)?;
            mgr.context_messages()
        };
        self.state.lock().await.messages = messages;
        Ok(())
    }

    pub async fn get_active_selection(&self) -> ModelSelection {
        self.state.lock().await.active_selection.clone()
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
        self.state.lock().await.active_selection = selection;
        Ok(())
    }

    pub async fn send_message(&self, content: String) -> Vec<Message> {
        dbg!("session.send_message.request", &content);
        {
            let mut mgr = self.session_manager.lock().await;
            let msg = mgr
                .append(Role::User, content)
                .map(|e| e.to_message())
                .unwrap_or_else(|_| Message::new(Role::Error, "failed to persist message"));
            let mut state = self.state.lock().await;
            state.messages.push(msg);
        }

        let context = {
            let mgr = self.session_manager.lock().await;
            mgr.context_messages()
        };
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
            let mut mgr = self.session_manager.lock().await;
            let _ = mgr.append(response.role, response.content.clone());
        }
        let mut state = self.state.lock().await;
        state.messages.push(response);
        dbg!(
            "session.send_message.response_messages_len",
            state.messages.len()
        );
        state.messages.clone()
    }

    pub async fn stream_message(&self, req: StreamMessageRequest) -> Result<PendingMessage> {
        stream_message(
            req,
            &self.project_path,
            &self.session_manager,
            &self.state,
            &self.event_bus,
            &self.agent_factory,
            &self.is_streaming,
            &self.cancel_tx,
        )
        .await
    }

    pub async fn interrupt_stream(&self, message_id: String) -> bool {
        interrupt_stream(
            message_id,
            &self.state,
            &self.event_bus,
            &self.is_streaming,
            &self.cancel_tx,
        )
        .await
    }
}

#[derive(Clone)]
pub struct AppService {
    registry: Arc<ProjectRegistry>,
    sessions: Arc<Mutex<HashMap<String, Arc<ActiveSession>>>>,
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
        // Verify the project is registered
        let abs = project_path.canonicalize().map_err(|_| {
            anyhow::anyhow!("project path does not exist: {}", project_path.display())
        })?;
        let projects = self.registry.list()?;
        if !projects.contains(&abs) {
            return Err(anyhow::anyhow!("project not registered: {}", abs.display()));
        }
        let id = Uuid::new_v4().to_string();
        SessionRegistry::new(&abs)?.session_log(&id)?;
        Ok(id)
    }

    pub fn list_sessions(&self, project_path: &std::path::Path) -> Result<Vec<SessionInfo>> {
        let abs = project_path.canonicalize().map_err(|_| {
            anyhow::anyhow!("project path does not exist: {}", project_path.display())
        })?;
        SessionRegistry::new(&abs)?.list()
    }

    /// Returns an `Arc<ActiveSession>`, creating it in memory if needed.
    /// Returns an error if the session does not exist on disk.
    pub async fn get_or_load_session(
        &self,
        project_path_str: &str,
        session_id: &str,
    ) -> Result<Arc<ActiveSession>> {
        let project_path = std::path::Path::new(project_path_str);
        let abs = project_path
            .canonicalize()
            .map_err(|_| anyhow::anyhow!("project path does not exist: {}", project_path_str))?;

        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(session_id) {
            return Ok(session.clone());
        }

        let session_manager = SessionManager::load(&abs, session_id)?;
        let messages = session_manager.context_messages();
        let default_selection = self
            .agent_factory
            .catalog()
            .default_selection()
            .expect("provider catalog must be valid");

        let session = Arc::new(ActiveSession::new(
            abs,
            session_manager,
            messages,
            self.agent_factory.clone(),
            default_selection,
        ));
        sessions.insert(session_id.to_string(), session.clone());
        Ok(session)
    }

    /// Called when a client disconnects. Removes the session from the in-memory map
    /// if no other clients hold a reference to it (Arc strong count == 1 means only the map holds it).
    pub async fn release_session(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(session_id) {
            // Arc::strong_count: map holds 1, caller holds 1 while checking.
            // After this function returns the caller's ref will be dropped.
            // If count is 2, no other client holds it → safe to evict.
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
    use crate::session::manager::SessionManager;
    use crate::session::tree::Branch;
    use tempfile::TempDir;

    fn project_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".gantry").join("sessions")).unwrap();
        tmp
    }

    fn entries_from_mgr(mgr: &SessionManager) -> HashMap<String, LogEntry> {
        mgr.all_entries()
            .map(|e| (e.id().to_string(), e.clone()))
            .collect()
    }

    fn build_branch_for_test(mgr: &SessionManager) -> (Branch, HashMap<String, LogEntry>) {
        let entries = entries_from_mgr(mgr);
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
        let mut mgr = SessionManager::create(tmp.path()).unwrap();
        mgr.append(Role::User, "root".into()).unwrap();
        mgr.append(Role::Assistant, "mid".into()).unwrap();
        mgr.append(Role::User, "leaf".into()).unwrap();

        let (branch, _) = build_branch_for_test(&mgr);

        assert_eq!(branch.depth, 0);
        assert_eq!(branch.nodes.len(), 3);
        assert!(branch.nodes.iter().all(|n| n.branches.is_empty()));
    }

    #[test]
    fn build_branch_two_children() {
        let tmp = project_dir();
        let mut mgr = SessionManager::create(tmp.path()).unwrap();
        let root_id = mgr
            .append(Role::User, "root".into())
            .unwrap()
            .base
            .id
            .clone();
        mgr.append(Role::Assistant, "child A".into()).unwrap();
        mgr.branch(&root_id).unwrap();
        mgr.append(Role::Assistant, "child B".into()).unwrap();

        let (branch, _) = build_branch_for_test(&mgr);

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
        let mut mgr = SessionManager::create(tmp.path()).unwrap();
        mgr.append(Role::User, "root".into()).unwrap();
        let mid_id = mgr
            .append(Role::Assistant, "mid".into())
            .unwrap()
            .base
            .id
            .clone();
        mgr.append(Role::User, "child A".into()).unwrap();
        mgr.branch(&mid_id).unwrap();
        mgr.append(Role::User, "child B".into()).unwrap();

        let (branch, _) = build_branch_for_test(&mgr);

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
        let mut mgr = SessionManager::create(tmp.path()).unwrap();
        let root_id = mgr
            .append(Role::User, "root".into())
            .unwrap()
            .base
            .id
            .clone();
        // Branch 1: root -> A -> fork(B, C)
        mgr.append(Role::Assistant, "A".into()).unwrap();
        let a_id = mgr.current_leaf_id.clone().unwrap();
        mgr.append(Role::User, "B".into()).unwrap();
        mgr.branch(&a_id).unwrap();
        mgr.append(Role::User, "C".into()).unwrap();
        // Branch 2: root -> D
        mgr.branch(&root_id).unwrap();
        mgr.append(Role::Assistant, "D".into()).unwrap();

        let (branch, _) = build_branch_for_test(&mgr);

        // root has 2 children (A and D) → branches list of 2
        assert_eq!(branch.nodes.len(), 1);
        assert_eq!(branch.nodes[0].branches.len(), 2);

        // One sub-branch contains A (depth=1) which itself forks into B and C
        let sub_with_a = branch.nodes[0]
            .branches
            .iter()
            .find(|b| b.nodes[0].content == "A")
            .unwrap();
        assert_eq!(sub_with_a.depth, 1);
        assert_eq!(sub_with_a.nodes[0].branches.len(), 2);
        assert_eq!(sub_with_a.nodes[0].branches[0].depth, 2);

        // Other sub-branch contains D (depth=1) with no children
        let sub_with_d = branch.nodes[0]
            .branches
            .iter()
            .find(|b| b.nodes[0].content == "D")
            .unwrap();
        assert_eq!(sub_with_d.depth, 1);
        assert!(sub_with_d.nodes[0].branches.is_empty());
    }
}
