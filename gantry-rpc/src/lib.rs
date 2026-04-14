mod client;
pub mod server;

pub use gantry_core::{
    AppEvent, Message, PendingMessage, ProjectInfo, SelectFormRequest, SelectFormResponse,
    SessionInfo, StreamMessageRequest,
};
use jsonrpsee::core::{RpcResult, SubscriptionResult};
use jsonrpsee::proc_macros::rpc;

pub use client::{JsonRpcClient, WsConnectionEvent};

#[rpc(client, server)]
pub trait GantryRpc {
    // --- project & session management ---

    #[method(name = "register_project")]
    async fn register_project(&self, path: String) -> RpcResult<()>;

    #[method(name = "list_projects")]
    async fn list_projects(&self) -> RpcResult<Vec<ProjectInfo>>;

    #[method(name = "create_session")]
    async fn create_session(&self, project_path: String) -> RpcResult<String>;

    #[method(name = "list_sessions")]
    async fn list_sessions(&self, project_path: String) -> RpcResult<Vec<SessionInfo>>;

    /// Bind this connection to a session. Must be called before any message methods.
    #[method(name = "connect_session")]
    async fn connect_session(&self, session_id: String, project_path: String) -> RpcResult<()>;

    // --- messaging (require connect_session first) ---

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

    #[method(name = "select_form")]
    async fn select_form(&self, req: SelectFormRequest) -> RpcResult<SelectFormResponse>;

    #[method(name = "get_messages")]
    async fn get_messages(&self) -> RpcResult<Vec<Message>>;

    #[method(name = "clear_messages")]
    async fn clear_messages(&self) -> RpcResult<()>;

    #[method(name = "interrupt_stream")]
    async fn interrupt_stream(&self, message_id: String) -> RpcResult<bool>;

    #[method(name = "ping")]
    async fn ping(&self) -> RpcResult<()>;
}
