mod client;
pub mod server;

pub use gantry_core::{
    AppEvent, Branch, BranchNode, Message, PendingMessage, SessionInfo, SessionTree,
    StreamMessageRequest,
};
use jsonrpsee::core::{RpcResult, SubscriptionResult};
use jsonrpsee::proc_macros::rpc;
use std::path::PathBuf;

pub use client::{JsonRpcClient, WsConnectionEvent};

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
    async fn create_session(&self, project_path: PathBuf) -> RpcResult<String>;

    #[method(name = "list_sessions")]
    async fn list_sessions(&self, project_path: PathBuf) -> RpcResult<Vec<SessionInfo>>;

    /// Bind this connection to a chat session. Must be called before any message methods.
    #[method(name = "bind_session")]
    async fn bind_session(&self, session_id: String, project_path: PathBuf) -> RpcResult<()>;

    // --- messaging (require bind_session first) ---

    #[method(name = "send_message")]
    async fn send_message(&self, content: String) -> RpcResult<Vec<Message>>;

    #[method(name = "stream_message")]
    async fn stream_message(&self, req: StreamMessageRequest) -> RpcResult<PendingMessage>;

    #[subscription(
        name = "subscribe_events" => "events",
        unsubscribe = "unsubscribe_events",
        item = AppEvent
    )]
    async fn subscribe_events(&self) -> SubscriptionResult;

    #[method(name = "get_messages")]
    async fn get_messages(&self) -> RpcResult<Vec<Message>>;

    #[method(name = "clear_messages")]
    async fn clear_messages(&self) -> RpcResult<()>;

    #[method(name = "interrupt_stream")]
    async fn interrupt_stream(&self, message_id: String) -> RpcResult<bool>;

    #[method(name = "ping")]
    async fn ping(&self) -> RpcResult<()>;

    #[method(name = "get_tree")]
    async fn get_tree(&self) -> RpcResult<gantry_core::SessionTree>;

    #[method(name = "branch")]
    async fn branch(&self, entry_id: String) -> RpcResult<()>;
}
