use std::future::Future;
use std::pin::Pin;

use anyhow::Result;
use futures::{Stream, StreamExt};
use rig::agent::{MultiTurnStreamItem, StreamingError, StreamingResult};
use rig::completion::CompletionModel;
use rig::message::Message;
use rig::streaming::StreamedAssistantContent;
use rig::wasm_compat::WasmCompatSend;

/// A heap-allocated, type-erased agent.
pub type BoxedAgent = Box<dyn AgentT>;

/// Object-safe interface over a fully-configured rig agent.
///
/// Abstracts over the concrete model and hook types so callers hold a `BoxedAgent` without
/// carrying any provider-specific generics.
pub trait AgentT: Send + Sync {
    /// Streams a multi-turn chat, returning a provider-agnostic [`ChatStream`].
    fn stream_chat(
        &self,
        prompt: Message,
        history: Vec<Message>,
    ) -> Pin<Box<dyn Future<Output = ChatStream> + Send + '_>>;
}

/// A pinned, boxed, provider-agnostic stream of [`ChatStreamItem`]s.
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatStreamItem, StreamingError>> + Send>>;

/// Provider-agnostic stream item. The `Final` variant inside [`StreamedAssistantContent`] carries
/// `()` because the raw provider chunk is not useful to callers; the assembled result is in
/// [`MultiTurnStreamItem::FinalResponse`].
pub type ChatStreamItem = MultiTurnStreamItem<()>;

impl<M, P> AgentT for rig::agent::Agent<M, P>
where
    M: CompletionModel + Send + Sync + 'static,
    M::StreamingResponse: WasmCompatSend,
    P: rig::agent::PromptHook<M> + Send + Sync + 'static,
{
    fn stream_chat(
        &self,
        prompt: Message,
        history: Vec<Message>,
    ) -> Pin<Box<dyn Future<Output = ChatStream> + Send + '_>> {
        Box::pin(async move {
            let stream: StreamingResult<M::StreamingResponse> =
                rig::streaming::StreamingChat::stream_chat(self, prompt, history).await;
            into_chat_stream(stream)
        })
    }
}

/// Wraps a [`StreamingResult`] into a [`ChatStream`] by erasing provider-specific payloads.
pub(super) fn into_chat_stream<R: 'static>(stream: StreamingResult<R>) -> ChatStream {
    Box::pin(
        stream.map(|item: Result<MultiTurnStreamItem<R>, StreamingError>| item.map(erase_final)),
    )
}

/// Maps a provider-typed [`MultiTurnStreamItem<R>`] to [`ChatStreamItem`] by erasing the
/// provider-specific `Final(R)` payload to `Final(())`.
fn erase_final<R>(item: MultiTurnStreamItem<R>) -> ChatStreamItem {
    match item {
        MultiTurnStreamItem::StreamAssistantItem(content) => {
            let erased = match content {
                StreamedAssistantContent::Final(_) => StreamedAssistantContent::Final(()),
                StreamedAssistantContent::Text(t) => StreamedAssistantContent::Text(t),
                StreamedAssistantContent::ToolCall {
                    tool_call,
                    internal_call_id,
                } => StreamedAssistantContent::ToolCall {
                    tool_call,
                    internal_call_id,
                },
                StreamedAssistantContent::ToolCallDelta {
                    id,
                    internal_call_id,
                    content,
                } => StreamedAssistantContent::ToolCallDelta {
                    id,
                    internal_call_id,
                    content,
                },
                StreamedAssistantContent::Reasoning(r) => StreamedAssistantContent::Reasoning(r),
                StreamedAssistantContent::ReasoningDelta { id, reasoning } => {
                    StreamedAssistantContent::ReasoningDelta { id, reasoning }
                }
            };
            MultiTurnStreamItem::StreamAssistantItem(erased)
        }
        MultiTurnStreamItem::StreamUserItem(u) => MultiTurnStreamItem::StreamUserItem(u),
        MultiTurnStreamItem::FinalResponse(f) => MultiTurnStreamItem::FinalResponse(f),
        // MultiTurnStreamItem is non-exhaustive; new variants are surfaced as FinalResponse::empty.
        _ => MultiTurnStreamItem::FinalResponse(rig::agent::FinalResponse::empty()),
    }
}
