use anyhow::Result;
use gantry_core::{AppEvent, AppService, Message, PendingMessage, SessionInfo, StreamMessageRequest};
use jsonrpsee::RpcModule;
use jsonrpsee::core::{RpcResult, SubscriptionResult, async_trait};
use jsonrpsee::server::{
    PendingSubscriptionSink, ServerBuilder, ServerConfig, ServerHandle, SubscriptionSink,
};
use jsonrpsee::types::ErrorObjectOwned;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::GantryRpcServer;

fn internal_error(details: impl Into<String>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32603, "Internal error", Some(details.into()))
}

fn invalid_request(msg: impl Into<String>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32600, "Invalid request", Some(msg.into()))
}

// Per-connection state. `RpcApp` is cloned per connection by jsonrpsee.
#[derive(Clone)]
pub struct RpcApp {
    app: AppService,
    /// (session_id, project_path) — set by bind_session
    session: Arc<Mutex<Option<(String, String)>>>,
}

impl RpcApp {
    fn new(app: AppService) -> Self {
        Self {
            app,
            session: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl GantryRpcServer for RpcApp {
    async fn register_project(&self, path: PathBuf) -> RpcResult<()> {
        dbg!("rpc.register_project.request", &path);
        self.app
            .register_project(&path)
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.register_project.done", &path);
        Ok(())
    }

    async fn list_projects(&self) -> RpcResult<Vec<PathBuf>> {
        dbg!("rpc.list_projects.request");
        let projects = self
            .app
            .list_projects()
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.list_projects.count", projects.len());
        Ok(projects)
    }

    async fn unregister_project(&self, path: PathBuf) -> RpcResult<()> {
        dbg!("rpc.unregister_project.request", &path);
        self.app
            .unregister_project(&path)
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.unregister_project.done", &path);
        Ok(())
    }

    async fn create_session(&self, project_path: PathBuf) -> RpcResult<String> {
        dbg!("rpc.create_session.request", &project_path);
        let id = self
            .app
            .create_session(&project_path)
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.create_session.created", &id);
        Ok(id)
    }

    async fn list_sessions(&self, project_path: PathBuf) -> RpcResult<Vec<SessionInfo>> {
        dbg!("rpc.list_sessions.request", &project_path);
        let sessions = self
            .app
            .list_sessions(&project_path)
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.list_sessions.count", sessions.len());
        Ok(sessions)
    }

    async fn bind_session(&self, session_id: String, project_path: PathBuf) -> RpcResult<()> {
        dbg!("rpc.bind_session.request", &session_id, &project_path);
        let project_path_str = project_path.to_string_lossy().into_owned();
        // Validate: load (or verify) the session exists
        self.app
            .get_or_load_session(&project_path_str, &session_id)
            .await
            .map_err(|e| invalid_request(e.to_string()))?;

        *self.session.lock().await = Some((session_id.clone(), project_path_str));
        dbg!("rpc.bind_session.done", &session_id);
        Ok(())
    }

    async fn send_message(&self, content: String) -> RpcResult<Vec<Message>> {
        dbg!("rpc.send_message.request", &content);
        let (session_id, project_path) = self
            .session
            .lock()
            .await
            .clone()
            .ok_or_else(|| invalid_request("no session selected; call bind_session first"))?;

        let session = self
            .app
            .get_or_load_session(&project_path, &session_id)
            .await
            .map_err(|e| internal_error(e.to_string()))?;

        let messages = session.send_message(content).await;
        dbg!("rpc.send_message.response_count", messages.len());
        Ok(messages)
    }

    async fn stream_message(&self, req: StreamMessageRequest) -> RpcResult<PendingMessage> {
        dbg!("rpc.stream_message.request.content", &req.content);
        let (session_id, project_path) = self
            .session
            .lock()
            .await
            .clone()
            .ok_or_else(|| invalid_request("no session selected; call bind_session first"))?;

        let session = self
            .app
            .get_or_load_session(&project_path, &session_id)
            .await
            .map_err(|e| internal_error(e.to_string()))?;

        let pending = session
            .stream_message(req)
            .await
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!(
            "rpc.stream_message.response.pending",
            &pending.id,
            &pending.content
        );
        Ok(pending)
    }

    async fn subscribe_events(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
        dbg!("rpc.subscribe_events.request");

        let (session_id, project_path) = {
            self.session
                .lock()
                .await
                .clone()
                .ok_or("no session selected; call bind_session first")?
        };

        let session = self
            .app
            .get_or_load_session(&project_path, &session_id)
            .await
            .map_err(|e| e.to_string())?;

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

    async fn get_messages(&self) -> RpcResult<Vec<Message>> {
        dbg!("rpc.get_messages.request");
        let (session_id, project_path) = self
            .session
            .lock()
            .await
            .clone()
            .ok_or_else(|| invalid_request("no session selected; call bind_session first"))?;

        let session = self
            .app
            .get_or_load_session(&project_path, &session_id)
            .await
            .map_err(|e| internal_error(e.to_string()))?;

        let messages = session.get_messages().await;
        dbg!("rpc.get_messages.response_count", messages.len());
        Ok(messages)
    }

    async fn clear_messages(&self) -> RpcResult<()> {
        dbg!("rpc.clear_messages.request");
        let (session_id, project_path) = self
            .session
            .lock()
            .await
            .clone()
            .ok_or_else(|| invalid_request("no session selected; call bind_session first"))?;

        let session = self
            .app
            .get_or_load_session(&project_path, &session_id)
            .await
            .map_err(|e| internal_error(e.to_string()))?;

        let _ = session;
        dbg!("rpc.clear_messages.done");
        Ok(())
    }

    async fn interrupt_stream(&self, message_id: String) -> RpcResult<bool> {
        dbg!("rpc.interrupt_stream.request", &message_id);
        let (session_id, project_path) = self
            .session
            .lock()
            .await
            .clone()
            .ok_or_else(|| invalid_request("no session selected; call bind_session first"))?;

        let session = self
            .app
            .get_or_load_session(&project_path, &session_id)
            .await
            .map_err(|e| internal_error(e.to_string()))?;

        let result = session.interrupt_stream(message_id).await;
        dbg!("rpc.interrupt_stream.response", result);
        Ok(result)
    }

    async fn ping(&self) -> RpcResult<()> {
        dbg!("rpc.ping.request");
        Ok(())
    }

    async fn get_tree(&self) -> RpcResult<gantry_core::SessionTree> {
        dbg!("rpc.get_tree.request");
        let (session_id, project_path) = self
            .session
            .lock()
            .await
            .clone()
            .ok_or_else(|| invalid_request("no session selected; call bind_session first"))?;

        let session = self
            .app
            .get_or_load_session(&project_path, &session_id)
            .await
            .map_err(|e| internal_error(e.to_string()))?;

        let branch = session.get_tree().await;
        Ok(branch)
    }

    async fn branch(&self, entry_id: String) -> RpcResult<()> {
        dbg!("rpc.branch.request", &entry_id);
        let (session_id, project_path) = self
            .session
            .lock()
            .await
            .clone()
            .ok_or_else(|| invalid_request("no session selected; call bind_session first"))?;

        let session = self
            .app
            .get_or_load_session(&project_path, &session_id)
            .await
            .map_err(|e| internal_error(e.to_string()))?;

        session
            .branch(entry_id)
            .await
            .map_err(|e| internal_error(e.to_string()))?;
        dbg!("rpc.branch.done");
        Ok(())
    }
}

pub async fn start_rpc_server<Context>(
    addr: &str,
    port: u16,
    module: RpcModule<Context>,
) -> Result<ServerHandle>
where
    Context: Send + Sync + 'static,
{
    dbg!("rpc.start_server", addr, port);
    let config = ServerConfig::builder().ws_only().build();
    let rpc_server = ServerBuilder::new()
        .set_config(config)
        .build((addr, port))
        .await?;
    dbg!("rpc.server_listening", addr, port);
    Ok(rpc_server.start(module))
}

pub async fn start_app_rpc_server(addr: &str, port: u16, app: AppService) -> Result<ServerHandle> {
    start_rpc_server(addr, port, RpcApp::new(app).into_rpc().remove_context()).await
}

async fn send_event(sink: &SubscriptionSink, event: &AppEvent) -> SubscriptionResult {
    let Ok(payload) = serde_json::value::to_raw_value(event) else {
        dbg!("rpc.send_event.serialize_failed");
        return Err("failed to serialize event".into());
    };
    sink.send(payload).await.map_err(|e| e.to_string())?;
    dbg!("rpc.send_event.sent", true, event);
    Ok(())
}
