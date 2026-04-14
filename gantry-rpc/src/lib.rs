mod client;
pub mod server;

pub use gantry_core::{
    AppEvent, Command, Message, PendingMessage, SelectFormRequest, SelectFormResponse,
    StreamMessageRequest,
};
use jsonrpsee::core::{RpcResult, SubscriptionResult};
use jsonrpsee::proc_macros::rpc;

pub use client::{JsonRpcClient, WsConnectionEvent};

#[rpc(client, server)]
pub trait GantryRpc {
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

    #[method(name = "get_commands")]
    async fn get_commands(&self) -> RpcResult<Vec<Command>>;
}
