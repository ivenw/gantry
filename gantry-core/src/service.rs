use crate::agent_factory::RigAgentFactory;
use crate::event_bus::EventBus;
use crate::project_registry::ProjectRegistry;
use crate::session::manager::SessionManager;
use crate::session::store::SessionStore;
use crate::state::ConversationState;
use crate::{
    AppEvent, ErrorEvent, FormHiddenEvent, FormShownEvent, InitEvent, Message,
    MessageReceivedEvent, ModelId, ModelSelection, PendingClearedEvent, PendingMessage, ProviderId,
    Role, SelectFormResponse, SessionInfo, StreamEndEvent, StreamMessageRequest, StreamStartEvent,
    TokenEvent,
};
use anyhow::Result;
use rig::message::Message as RigMessage;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tokio::sync::{Mutex, broadcast, oneshot};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ActiveSession — per-session in-memory state
// ---------------------------------------------------------------------------

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
            form: state.active_form.clone(),
        })
    }

    pub async fn get_messages(&self) -> Vec<Message> {
        self.state.lock().await.messages.clone()
    }

    pub async fn clear_messages(&self) {
        self.state.lock().await.messages.clear();
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
        let mut rig_messages = Self::to_rig_messages(context);
        dbg!("session.send_message.snapshot_len", rig_messages.len());
        let response = match rig_messages.pop() {
            Some(prompt) => match self.agent_factory.agent(&selection).await {
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
        dbg!("session.stream_message.request", &req.content);
        if self
            .is_streaming
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(anyhow::anyhow!("a stream is already in progress"));
        }
        let _streaming_guard = StreamingGuard {
            is_streaming: self.is_streaming.clone(),
        };

        let pending = PendingMessage::new(req.content.clone());

        {
            let mut mgr = self.session_manager.lock().await;
            let msg = mgr
                .append(Role::User, req.content)
                .map(|e| e.to_message())
                .unwrap_or_else(|_| Message::new(Role::Error, "failed to persist message"));
            let mut state = self.state.lock().await;
            state.messages.push(msg);
            state.pending_message = Some(pending.clone());
        }

        self.event_bus
            .publish(AppEvent::MessageReceived(MessageReceivedEvent {
                id: pending.id.clone(),
                content: pending.content.clone(),
            }));
        dbg!("session.stream_message.pending_published", &pending.id);

        let snapshot = self.get_messages().await;
        let selection = self.get_active_selection().await;
        let mut rig_messages = Self::to_rig_messages(snapshot);
        let Some(prompt) = rig_messages.pop() else {
            self.clear_pending(&pending.id).await;
            self.event_bus.publish(AppEvent::Error(ErrorEvent {
                message: "cannot generate tokens with empty message history".to_string(),
            }));
            return Ok(pending);
        };

        dbg!(
            "session.stream_message.snapshot_len",
            rig_messages.len() + 1
        );
        let message_id = Uuid::new_v4().to_string();
        self.event_bus
            .publish(AppEvent::StreamStart(StreamStartEvent {
                message_id: message_id.clone(),
                pending_of: pending.id.clone(),
            }));

        let (token_tx, mut token_rx) = mpsc::channel(128);
        let (cancel_tx, mut cancel_rx) = oneshot::channel();
        *self.cancel_tx.lock().await = Some(cancel_tx);

        let agent = self.agent_factory.agent(&selection).await;
        let llm_task = tokio::spawn(async move {
            let agent = agent?;
            agent.stream_chat(prompt, rig_messages, token_tx).await
        });

        let mut accumulated = String::new();
        let mut token_count = 0usize;
        let mut cancelled = false;
        let mut line_buffer = String::new();
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    dbg!("session.stream_message.cancelled");
                    cancelled = true;
                    break;
                }
                token_opt = token_rx.recv() => {
                    match token_opt {
                        Some(token) => {
                            accumulated.push_str(&token);
                            token_count += 1;
                            line_buffer.push_str(&token);

                            while let Some(newline_idx) = line_buffer.find('\n') {
                                let line = line_buffer.drain(..=newline_idx).collect::<String>();
                                self.event_bus.publish(AppEvent::Token(TokenEvent {
                                    message_id: message_id.clone(),
                                    delta: line,
                                }));
                            }
                        }
                        None => break,
                    }
                }
            }
        }

        if !line_buffer.is_empty() {
            self.event_bus.publish(AppEvent::Token(TokenEvent {
                message_id: message_id.clone(),
                delta: line_buffer,
            }));
        }

        if cancelled {
            dbg!("session.stream_message.was_cancelled");
            self.is_streaming.store(false, Ordering::SeqCst);
            return Ok(pending);
        }

        match llm_task.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                dbg!("session.stream_message.llm_err", err.to_string());
                self.clear_pending(&pending.id).await;
                self.event_bus.publish(AppEvent::Error(ErrorEvent {
                    message: err.to_string(),
                }));
                return Ok(pending);
            }
            Err(err) => {
                dbg!("session.stream_message.llm_join_err", err.to_string());
                self.clear_pending(&pending.id).await;
                self.event_bus.publish(AppEvent::Error(ErrorEvent {
                    message: format!("llm task failed: {}", err),
                }));
                return Ok(pending);
            }
        }

        dbg!("session.stream_message.tokens_received", token_count);
        dbg!("session.stream_message.accumulated_len", accumulated.len());

        self.event_bus.publish(AppEvent::StreamEnd(StreamEndEvent {
            message_id,
            content: accumulated.clone(),
        }));
        dbg!("session.stream_message.end_published");

        {
            let mut mgr = self.session_manager.lock().await;
            let msg = mgr
                .append(Role::Assistant, accumulated)
                .map(|e| e.to_message())
                .unwrap_or_else(|_| Message::new(Role::Error, "failed to persist message"));
            let mut state = self.state.lock().await;
            state.messages.push(msg);
        }

        self.clear_pending(&pending.id).await;
        dbg!("session.stream_message.done", &pending.id);
        Ok(pending)
    }

    pub async fn select_form(&self, form_id: String, selection: String) -> SelectFormResponse {
        let maybe_form = { self.state.lock().await.active_form.clone() };

        if let Some(form) = maybe_form {
            if form.id == form_id {
                self.hide_form(form.id.clone(), "client".to_string(), selection.clone())
                    .await;
                return SelectFormResponse {
                    success: true,
                    selected_by: Some("client".to_string()),
                    message: Some(format!("Selected: {}", selection)),
                };
            }

            return SelectFormResponse {
                success: false,
                selected_by: None,
                message: Some("Form not found".to_string()),
            };
        }

        SelectFormResponse {
            success: false,
            selected_by: None,
            message: Some("No active form".to_string()),
        }
    }

    pub async fn interrupt_stream(&self, message_id: String) -> bool {
        dbg!("session.interrupt_stream", &message_id);

        if let Some(cancel_tx) = self.cancel_tx.lock().await.take() {
            let _ = cancel_tx.send(());
            dbg!("session.interrupt_stream.sent_cancel");
        }

        let state = self.state.lock().await;
        let pending = state.pending_message.clone();
        let accumulated = state
            .messages
            .last()
            .filter(|m| m.role == Role::Assistant)
            .map(|m| m.content.clone())
            .unwrap_or_default();

        if let Some(pending) = pending {
            drop(state);
            dbg!(
                "session.interrupt_stream.accumulated_len",
                accumulated.len()
            );

            if !accumulated.is_empty() {
                self.event_bus.publish(AppEvent::StreamEnd(StreamEndEvent {
                    message_id: message_id.clone(),
                    content: accumulated.clone(),
                }));
            }

            self.clear_pending(&pending.id).await;
        }

        self.is_streaming.store(false, Ordering::SeqCst);
        dbg!("session.interrupt_stream.done");
        true
    }

    pub async fn show_form(&self, options: Vec<String>) {
        let form = crate::FormState::new(options);
        {
            let mut state = self.state.lock().await;
            state.active_form = Some(form.clone());
        }
        self.event_bus.publish(AppEvent::FormShown(FormShownEvent {
            id: form.id,
            options: form.options,
        }));
    }

    pub async fn hide_form(&self, form_id: String, selected_by: String, selected: String) {
        {
            let mut state = self.state.lock().await;
            state.active_form = None;
        }

        self.event_bus
            .publish(AppEvent::FormHidden(FormHiddenEvent {
                id: form_id,
                selected_by,
                selected,
            }));
    }

    async fn clear_pending(&self, pending_id: &str) {
        dbg!("session.clear_pending", pending_id);
        {
            let mut state = self.state.lock().await;
            state.pending_message = None;
        }

        self.event_bus
            .publish(AppEvent::PendingCleared(PendingClearedEvent {
                pending_id: pending_id.to_string(),
            }));
    }

    fn to_rig_messages(messages: Vec<Message>) -> Vec<RigMessage> {
        messages
            .into_iter()
            .map(|msg| match msg.role {
                Role::User => RigMessage::user(msg.content),
                Role::Assistant => RigMessage::assistant(msg.content),
                Role::Error => RigMessage::user(format!("[Error]: {}", msg.content)),
            })
            .collect()
    }
}

struct StreamingGuard {
    is_streaming: Arc<AtomicBool>,
}

impl Drop for StreamingGuard {
    fn drop(&mut self) {
        self.is_streaming.store(false, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// AppService — project registry + session lifecycle management
// ---------------------------------------------------------------------------

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

    // --- project management ---

    pub fn register_project(&self, path: &std::path::Path) -> Result<()> {
        self.registry.register(path)
    }

    pub fn list_projects(&self) -> Result<Vec<PathBuf>> {
        self.registry.list()
    }

    pub fn unregister_project(&self, path: &Path) -> Result<()> {
        self.registry.unregister(path)
    }

    // --- session management ---

    pub fn create_session(&self, project_path: &std::path::Path) -> Result<String> {
        // Verify the project is registered
        let abs = project_path.canonicalize().map_err(|_| {
            anyhow::anyhow!("project path does not exist: {}", project_path.display())
        })?;
        let projects = self.registry.list()?;
        if !projects.contains(&abs) {
            return Err(anyhow::anyhow!("project not registered: {}", abs.display()));
        }
        SessionStore::create(&abs)
    }

    pub fn list_sessions(&self, project_path: &std::path::Path) -> Result<Vec<SessionInfo>> {
        let abs = project_path.canonicalize().map_err(|_| {
            anyhow::anyhow!("project path does not exist: {}", project_path.display())
        })?;
        SessionStore::list(&abs)
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

        if !SessionStore::exists(&abs, session_id) {
            return Err(anyhow::anyhow!("session not found: {}", session_id));
        }

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
