use anyhow::Result;
use gantry_core::dirs::{ProjectConfigDir, ProjectRootDir};
use gantry_core::project::ProjectRegistry;
use gantry_core::provider::agent_factory::RigAgentFactory;
use gantry_core::session::registry::{FsSessionRegistry, SessionRegistry};
use gantry_core::{
    AppEvent, ChatService, ErrorEvent, InitEvent, MessageReceivedEvent, PendingClearedEvent,
    ProviderConfig, SessionId, SessionInfo, StreamEndEvent, StreamEvent, StreamMessageRequest,
    StreamStartEvent, TokenEvent, ToolCallStartedEvent, ToolResultReceivedEvent,
};
use gantry_rpc::wire::EventBus;
use gantry_rpc::wire::message::to_wire;
use gantry_rpc::{GantryRpcServer, WireMessage};
use gantry_rpc::{SessionHandle, SessionManager};
use jsonrpsee::core::{RpcResult, SubscriptionResult, async_trait};
use jsonrpsee::server::{PendingSubscriptionSink, SubscriptionSink};
use jsonrpsee::types::ErrorObjectOwned;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, broadcast, oneshot};
use uuid::Uuid;

fn internal_error(details: impl Into<String>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32603, "Internal error", Some(details.into()))
}

fn invalid_request(msg: impl Into<String>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32600, "Invalid request", Some(msg.into()))
}

/// Per-connection transport state wrapping a domain SessionHandle.
struct RpcSession {
    handle: Arc<SessionHandle>,
    chat_service: Arc<ChatService>,
    pending_id: Arc<Mutex<Option<String>>>,
    event_bus: EventBus,
    is_streaming: Arc<AtomicBool>,
    cancel_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl RpcSession {
    /// Creates a new RPC session wrapping the given domain session handle and chat service.
    fn new(handle: Arc<SessionHandle>, chat_service: Arc<ChatService>) -> Self {
        Self {
            handle,
            chat_service,
            pending_id: Arc::new(Mutex::new(None)),
            event_bus: EventBus::new(1000),
            is_streaming: Arc::new(AtomicBool::new(false)),
            cancel_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Subscribes to the event bus for this session.
    fn subscribe_events(&self) -> broadcast::Receiver<AppEvent> {
        self.event_bus.subscribe()
    }

    /// Builds the init event with current messages and pending state.
    async fn init_event(&self) -> AppEvent {
        let messages = self.handle.get_messages().await;
        // pending_message on the wire is the user turn currently being streamed; we synthesize
        // a minimal wire-compatible message from the stored content if a stream is active.
        let pending_message = self
            .pending_id
            .lock()
            .await
            .clone()
            .map(|_| rig::message::Message::user(String::new()));
        AppEvent::Init(InitEvent {
            client_id: Uuid::new_v4().to_string(),
            messages,
            pending_message,
        })
    }

    /// Starts streaming a message and spawns a task to forward stream events to the event bus.
    async fn stream_message(&self, req: StreamMessageRequest) -> Result<String> {
        if self
            .is_streaming
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(anyhow::anyhow!("a stream is already in progress"));
        }

        let (pending_id, cancel_tx, mut event_rx) = self
            .chat_service
            .stream_message(self.handle.clone(), req)
            .await?;

        *self.pending_id.lock().await = Some(pending_id.clone());
        *self.cancel_tx.lock().await = Some(cancel_tx);

        let event_bus = self.event_bus.clone();
        let pending_id_state = self.pending_id.clone();
        let is_streaming = self.is_streaming.clone();

        tokio::spawn(async move {
            while let Some(ev) = event_rx.recv().await {
                match ev {
                    StreamEvent::MessageReceived {
                        content,
                        pending_id,
                    } => {
                        event_bus.publish(AppEvent::MessageReceived(MessageReceivedEvent {
                            id: pending_id,
                            content,
                        }));
                    }
                    StreamEvent::StreamStart {
                        message_id,
                        pending_id,
                    } => {
                        event_bus.publish(AppEvent::StreamStart(StreamStartEvent {
                            message_id,
                            pending_of: pending_id,
                        }));
                    }
                    StreamEvent::Token { message_id, delta } => {
                        event_bus.publish(AppEvent::Token(TokenEvent { message_id, delta }));
                    }
                    StreamEvent::StreamEnd {
                        message_id,
                        content,
                    } => {
                        event_bus.publish(AppEvent::StreamEnd(StreamEndEvent {
                            message_id,
                            content,
                        }));
                    }
                    StreamEvent::PendingCleared { pending_id } => {
                        *pending_id_state.lock().await = None;
                        event_bus
                            .publish(AppEvent::PendingCleared(PendingClearedEvent { pending_id }));
                    }
                    StreamEvent::ToolCallStarted {
                        tool_call_id,
                        tool_name,
                    } => {
                        event_bus.publish(AppEvent::ToolCallStarted(ToolCallStartedEvent {
                            tool_call_id,
                            tool_name,
                        }));
                    }
                    StreamEvent::ToolResultReceived {
                        tool_call_id,
                        tool_name,
                        content,
                    } => {
                        event_bus.publish(AppEvent::ToolResultReceived(ToolResultReceivedEvent {
                            tool_call_id,
                            tool_name,
                            content,
                        }));
                    }
                    StreamEvent::Error { message } => {
                        event_bus.publish(AppEvent::Error(ErrorEvent { message }));
                    }
                }
            }
            is_streaming.store(false, Ordering::SeqCst);
        });

        Ok(pending_id)
    }

    /// Cancels the active stream and publishes terminal events to subscribers.
    async fn interrupt_stream(&self, message_id: String) -> bool {
        if let Some(tx) = self.cancel_tx.lock().await.take() {
            let _ = tx.send(());
            dbg!("rpc_session.interrupt_stream.sent_cancel");
        }

        let pending = self.pending_id.lock().await.take();
        if let Some(pending_id) = pending {
            event_bus_publish_stream_end(&self.event_bus, message_id);
            self.event_bus
                .publish(AppEvent::PendingCleared(PendingClearedEvent { pending_id }));
        }

        self.is_streaming.store(false, Ordering::SeqCst);
        true
    }
}

fn event_bus_publish_stream_end(event_bus: &EventBus, message_id: String) {
    event_bus.publish(AppEvent::StreamEnd(StreamEndEvent {
        message_id,
        content: String::new(),
    }));
}

/// Per-connection RPC state. Cloned per connection by jsonrpsee.
#[derive(Clone)]
pub struct RpcApp<P> {
    projects: Arc<P>,
    sessions: Arc<SessionManager>,
    agent_factory: RigAgentFactory,
    chat_service: Arc<ChatService>,
    session: Arc<Mutex<Option<(SessionId, String)>>>,
    rpc_session: Arc<Mutex<Option<Arc<RpcSession>>>>,
}

impl<P: ProjectRegistry> RpcApp<P> {
    /// Creates a new RPC application binding the given domain dependencies.
    pub fn new(
        projects: Arc<P>,
        sessions: Arc<SessionManager>,
        agent_factory: RigAgentFactory,
    ) -> Self {
        let chat_service = Arc::new(ChatService::new(agent_factory.clone()));
        Self {
            projects,
            sessions,
            agent_factory,
            chat_service,
            session: Arc::new(Mutex::new(None)),
            rpc_session: Arc::new(Mutex::new(None)),
        }
    }

    async fn bound_session(&self) -> Result<Arc<RpcSession>, ErrorObjectOwned> {
        self.rpc_session
            .lock()
            .await
            .clone()
            .ok_or_else(|| invalid_request("no session selected; call bind_session first"))
    }
}

#[async_trait]
impl<P: ProjectRegistry + Send + Sync + 'static> GantryRpcServer for RpcApp<P> {
    async fn register_project(&self, path: PathBuf) -> RpcResult<()> {
        dbg!("rpc.register_project.request", &path);
        self.projects
            .register(&path)
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.register_project.done", &path);
        Ok(())
    }

    async fn list_projects(&self) -> RpcResult<Vec<PathBuf>> {
        dbg!("rpc.list_projects.request");
        let projects = self
            .projects
            .list()
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.list_projects.count", projects.len());
        Ok(projects)
    }

    async fn unregister_project(&self, path: PathBuf) -> RpcResult<()> {
        dbg!("rpc.unregister_project.request", &path);
        self.projects
            .unregister(&path)
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.unregister_project.done", &path);
        Ok(())
    }

    async fn create_session(&self, project_path: PathBuf) -> RpcResult<SessionId> {
        dbg!("rpc.create_session.request", &project_path);
        let default_selection = self
            .agent_factory
            .catalog()
            .default_selection()
            .expect("provider catalog must be valid");
        let id = self
            .sessions
            .create_session(&project_path, &*self.projects, default_selection)
            .await
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.create_session.created", &id);
        Ok(id)
    }

    async fn list_sessions(&self, project_path: PathBuf) -> RpcResult<Vec<SessionInfo>> {
        dbg!("rpc.list_sessions.request", &project_path);
        let sessions = ProjectRootDir::new(&project_path)
            .and_then(|root| ProjectConfigDir::new(&root))
            .and_then(|config_dir| FsSessionRegistry::new(&config_dir))
            .and_then(|r| r.list())
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.list_sessions.count", sessions.len());
        Ok(sessions)
    }

    async fn bind_session(&self, session_id: SessionId, project_path: PathBuf) -> RpcResult<()> {
        dbg!("rpc.bind_session.request", &session_id, &project_path);
        let project_path_str = project_path.to_string_lossy().into_owned();
        let default_selection = self
            .agent_factory
            .catalog()
            .default_selection()
            .expect("provider catalog must be valid");
        let handle = self
            .sessions
            .get_or_load(&project_path_str, &session_id, default_selection)
            .await
            .map_err(|e| invalid_request(e.to_string()))?;

        *self.session.lock().await = Some((session_id.clone(), project_path_str));
        *self.rpc_session.lock().await =
            Some(Arc::new(RpcSession::new(handle, self.chat_service.clone())));
        dbg!("rpc.bind_session.done", &session_id);
        Ok(())
    }

    async fn send_message(&self, content: String) -> RpcResult<Vec<WireMessage>> {
        dbg!("rpc.send_message.request", &content);
        let session = self.bound_session().await?;
        let messages = session
            .chat_service
            .send_message(session.handle.clone(), content)
            .await
            .map_err(|e| jsonrpsee::types::ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        dbg!("rpc.send_message.response_count", messages.len());
        Ok(messages.iter().filter_map(to_wire).collect())
    }

    async fn stream_message(&self, req: StreamMessageRequest) -> RpcResult<String> {
        dbg!("rpc.stream_message.request.content", &req.content);
        let session = self.bound_session().await?;
        let pending_id = session
            .stream_message(req)
            .await
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.stream_message.response.pending", &pending_id);
        Ok(pending_id)
    }

    async fn subscribe_events(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
        dbg!("rpc.subscribe_events.request");

        let session = self
            .rpc_session
            .lock()
            .await
            .clone()
            .ok_or("no session selected; call bind_session first")?;

        let sink = pending.accept().await.map_err(|e| e.to_string())?;
        dbg!("rpc.subscribe_events.accepted");

        let init_event = session.init_event().await;
        if let Err(err) = send_event(&sink, &init_event).await {
            dbg!("rpc.subscribe_events.init_send_failed", &err);
            return Ok(());
        }
        dbg!("rpc.subscribe_events.init_sent");

        let mut event_rx = session.subscribe_events();
        loop {
            tokio::select! {
                _ = sink.closed() => break,
                event = event_rx.recv() => {
                    match event {
                        Ok(ev) => {
                            dbg!("rpc.subscribe_events.broadcast_event", &ev);
                            if let Err(err) = send_event(&sink, &ev).await {
                                dbg!("rpc.subscribe_events.broadcast_send_failed", &err);
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            dbg!("rpc.subscribe_events.lagged");
                            let catch_up = session.init_event().await;
                            if let Err(err) = send_event(&sink, &catch_up).await {
                                dbg!("rpc.subscribe_events.catchup_send_failed", &err);
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            dbg!("rpc.subscribe_events.closed");
                            break;
                        }
                    }
                }
            }
        }
        dbg!("rpc.subscribe_events.ended");
        Ok(())
    }

    async fn get_messages(&self) -> RpcResult<Vec<WireMessage>> {
        dbg!("rpc.get_messages.request");
        let session = self.bound_session().await?;
        let messages = session.handle.get_messages().await;
        dbg!("rpc.get_messages.response_count", messages.len());
        Ok(messages.iter().filter_map(to_wire).collect())
    }

    async fn clear_messages(&self) -> RpcResult<()> {
        dbg!("rpc.clear_messages.request");
        let _ = self.bound_session().await?;
        dbg!("rpc.clear_messages.done");
        Ok(())
    }

    async fn interrupt_stream(&self, message_id: String) -> RpcResult<bool> {
        dbg!("rpc.interrupt_stream.request", &message_id);
        let session = self.bound_session().await?;
        let result = session.interrupt_stream(message_id).await;
        dbg!("rpc.interrupt_stream.response", result);
        Ok(result)
    }

    async fn list_providers(&self) -> RpcResult<Vec<ProviderConfig>> {
        dbg!("rpc.list_providers.request");
        Ok(self.chat_service.list_providers())
    }

    async fn set_active_provider(&self, provider_id: gantry_core::ProviderId) -> RpcResult<()> {
        dbg!("rpc.set_active_provider.request", &provider_id);
        let session = self.bound_session().await?;
        self.chat_service
            .set_active_provider(&session.handle, provider_id)
            .await
            .map_err(|e| internal_error(e.to_string()))
    }

    async fn set_active_model(&self, model_id: gantry_core::ModelId) -> RpcResult<()> {
        dbg!("rpc.set_active_model.request", &model_id);
        let session = self.bound_session().await?;
        self.chat_service
            .set_active_model(&session.handle, model_id)
            .await
            .map_err(|e| internal_error(e.to_string()))
    }

    async fn ping(&self) -> RpcResult<()> {
        dbg!("rpc.ping.request");
        Ok(())
    }

    async fn get_tree(&self) -> RpcResult<Option<gantry_core::SessionTree>> {
        dbg!("rpc.get_tree.request");
        let session = self.bound_session().await?;
        Ok(session.handle.get_tree().await)
    }

    async fn branch(&self, entry_id: String) -> RpcResult<()> {
        dbg!("rpc.branch.request", &entry_id);
        let session = self.bound_session().await?;
        session
            .handle
            .branch(entry_id)
            .await
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.branch.done");
        Ok(())
    }
}

async fn send_event(sink: &SubscriptionSink, event: &AppEvent) -> SubscriptionResult {
    let wire = gantry_rpc::WireAppEvent::from(event);
    let Ok(payload) = serde_json::value::to_raw_value(&wire) else {
        dbg!("rpc.send_event.serialize_failed");
        return Err("failed to serialize event".into());
    };
    sink.send(payload).await.map_err(|e| e.to_string())?;
    dbg!("rpc.send_event.sent", true, event);
    Ok(())
}
