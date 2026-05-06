use anyhow::Result;
use futures::{Stream, StreamExt};
use rig::agent::{Agent, MultiTurnStreamItem, StreamingError};
use rig::message::Message;
use rig::completion::Chat;
use rig::providers::ollama;
use rig::streaming::{StreamedAssistantContent, StreamingChat};
use std::pin::Pin;

/// Provider-agnostic stream item. The `Final` variant inside [`StreamedAssistantContent`] carries
/// `()` because the raw provider chunk is not useful to callers; the assembled result is in
/// [`MultiTurnStreamItem::FinalResponse`].
pub type ChatStreamItem = MultiTurnStreamItem<()>;

/// A pinned, boxed, provider-agnostic stream of [`ChatStreamItem`]s.
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatStreamItem, StreamingError>> + Send>>;

/// Maps a provider-typed [`MultiTurnStreamItem<R>`] to [`ChatStreamItem`] by erasing the
/// provider-specific `Final(R)` payload to `Final(())`.
fn erase_final<R>(item: MultiTurnStreamItem<R>) -> ChatStreamItem {
    match item {
        MultiTurnStreamItem::StreamAssistantItem(content) => {
            let erased = match content {
                StreamedAssistantContent::Final(_) => StreamedAssistantContent::Final(()),
                StreamedAssistantContent::Text(t) => StreamedAssistantContent::Text(t),
                StreamedAssistantContent::ToolCall { tool_call, internal_call_id } => {
                    StreamedAssistantContent::ToolCall { tool_call, internal_call_id }
                }
                StreamedAssistantContent::ToolCallDelta { id, internal_call_id, content } => {
                    StreamedAssistantContent::ToolCallDelta { id, internal_call_id, content }
                }
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

/// A provider-agnostic handle to a configured, ready-to-use agent.
pub struct ConfiguredAgent {
    inner: ConfiguredAgentKind,
}

enum ConfiguredAgentKind {
    Ollama(Agent<ollama::CompletionModel>),
}

impl ConfiguredAgent {
    /// Wraps an Ollama agent.
    pub(super) fn ollama(agent: Agent<ollama::CompletionModel>) -> Self {
        Self {
            inner: ConfiguredAgentKind::Ollama(agent),
        }
    }

    /// Sends a single-turn chat and returns the assistant's response text.
    pub async fn chat(&self, prompt: Message, history: Vec<Message>) -> Result<String> {
        match &self.inner {
            ConfiguredAgentKind::Ollama(agent) => Ok(agent.chat(prompt, history).await?),
        }
    }

    /// Streams a multi-turn chat, returning a provider-agnostic [`ChatStream`].
    pub async fn stream_chat(&self, prompt: Message, history: Vec<Message>) -> ChatStream {
        match &self.inner {
            ConfiguredAgentKind::Ollama(agent) => Box::pin(
                agent
                    .stream_chat(prompt, history)
                    .await
                    .map(|item| item.map(erase_final)),
            ),
        }
    }
}
