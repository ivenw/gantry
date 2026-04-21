mod client;
pub mod session_manager;
pub mod wire;

pub use gantry_core::{
    AppEvent, Branch, ModelId, ProviderConfig, ProviderId, SessionId, SessionInfo, SessionTree,
    StreamMessageRequest, ToolCallStartedEvent, ToolResultReceivedEvent,
};
use jsonrpsee::core::{RpcResult, SubscriptionResult};
use jsonrpsee::proc_macros::rpc;
use std::path::PathBuf;

pub use client::{JsonRpcClient, WsConnectionEvent};
pub use session_manager::{SessionHandle, SessionManager};
pub use wire::{WireAppEvent, WireMessage};

#[rpc(client, server)]
pub trait GantryRpc {
    // --- project & session management ---

    #[method(name = "register_project")]
    async fn register_project(&self, path: PathBuf) -> RpcResult<()>;

    #[method(name = "list_projects")]
    async fn list_projects(&self) -> RpcResult<Vec<PathBuf>>;

    #[method(name = "unregister_project")]
    async fn unregister_project(&self, path: PathBuf) -> RpcResult<()>;

    #[method(name = "create_session")]
    async fn create_session(&self, project_path: PathBuf) -> RpcResult<SessionId>;

    #[method(name = "list_sessions")]
    async fn list_sessions(&self, project_path: PathBuf) -> RpcResult<Vec<SessionInfo>>;

    /// Bind this connection to a chat session. Must be called before any message methods.
    #[method(name = "bind_session")]
    async fn bind_session(&self, session_id: SessionId, project_path: PathBuf) -> RpcResult<()>;

    // --- messaging (require bind_session first) ---

    #[method(name = "send_message")]
    async fn send_message(&self, content: String) -> RpcResult<Vec<WireMessage>>;

    #[method(name = "stream_message")]
    async fn stream_message(&self, req: StreamMessageRequest) -> RpcResult<String>;

    #[subscription(
        name = "subscribe_events" => "events",
        unsubscribe = "unsubscribe_events",
        item = WireAppEvent
    )]
    async fn subscribe_events(&self) -> SubscriptionResult;

    #[method(name = "get_messages")]
    async fn get_messages(&self) -> RpcResult<Vec<WireMessage>>;

    #[method(name = "clear_messages")]
    async fn clear_messages(&self) -> RpcResult<()>;

    #[method(name = "interrupt_stream")]
    async fn interrupt_stream(&self, message_id: String) -> RpcResult<bool>;

    // --- model selection ---

    #[method(name = "list_providers")]
    async fn list_providers(&self) -> RpcResult<Vec<ProviderConfig>>;

    #[method(name = "set_active_provider")]
    async fn set_active_provider(&self, provider_id: ProviderId) -> RpcResult<()>;

    #[method(name = "set_active_model")]
    async fn set_active_model(&self, model_id: ModelId) -> RpcResult<()>;

    #[method(name = "ping")]
    async fn ping(&self) -> RpcResult<()>;

    #[method(name = "get_tree")]
    async fn get_tree(&self) -> RpcResult<Option<gantry_core::SessionTree>>;

    #[method(name = "branch")]
    async fn branch(&self, entry_id: String) -> RpcResult<()>;
}
