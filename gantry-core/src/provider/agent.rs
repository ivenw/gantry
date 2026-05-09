use anyhow::Result;
use futures::{Stream, StreamExt};
use rig::agent::{Agent, MultiTurnStreamItem, StreamingError, StreamingResult};
use rig::completion::Chat;
use rig::message::Message;
use rig::providers::{copilot, ollama, openai};
use rig::streaming::{StreamedAssistantContent, StreamingChat};
use std::pin::Pin;

/// A pinned, boxed, provider-agnostic stream of [`ChatStreamItem`]s.
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatStreamItem, StreamingError>> + Send>>;

/// Provider-agnostic stream item. The `Final` variant inside [`StreamedAssistantContent`] carries
/// `()` because the raw provider chunk is not useful to callers; the assembled result is in
/// [`MultiTurnStreamItem::FinalResponse`].
pub type ChatStreamItem = MultiTurnStreamItem<()>;

/// A provider-agnostic handle to a configured, ready-to-use agent.
pub struct ConfiguredAgent {
    inner: ConfiguredAgentKind,
}

enum ConfiguredAgentKind {
    Ollama(Agent<ollama::CompletionModel>),
    Copilot(Agent<copilot::CompletionModel>),
    OpenAiCompletions(Agent<openai::completion::CompletionModel>),
    OpenAiResponses(Agent<openai::responses_api::ResponsesCompletionModel>),
}

impl ConfiguredAgent {
    /// Wraps an Ollama agent.
    pub(super) fn ollama(agent: Agent<ollama::CompletionModel>) -> Self {
        Self {
            inner: ConfiguredAgentKind::Ollama(agent),
        }
    }

    /// Wraps a GitHub Copilot agent.
    pub(super) fn copilot(agent: Agent<copilot::CompletionModel>) -> Self {
        Self {
            inner: ConfiguredAgentKind::Copilot(agent),
        }
    }

    /// Wraps an OpenAI-compatible completions API agent.
    pub(super) fn openai_completions(agent: Agent<openai::completion::CompletionModel>) -> Self {
        Self {
            inner: ConfiguredAgentKind::OpenAiCompletions(agent),
        }
    }

    /// Wraps an OpenAI-compatible responses API agent.
    pub(super) fn openai_responses(
        agent: Agent<openai::responses_api::ResponsesCompletionModel>,
    ) -> Self {
        Self {
            inner: ConfiguredAgentKind::OpenAiResponses(agent),
        }
    }

    /// Sends a single-turn chat and returns the assistant's response text.
    pub async fn chat(&self, prompt: Message, history: Vec<Message>) -> Result<String> {
        match &self.inner {
            ConfiguredAgentKind::Ollama(agent) => Ok(agent.chat(prompt, history).await?),
            ConfiguredAgentKind::Copilot(agent) => Ok(agent.chat(prompt, history).await?),
            ConfiguredAgentKind::OpenAiCompletions(agent) => {
                Ok(agent.chat(prompt, history).await?)
            }
            ConfiguredAgentKind::OpenAiResponses(agent) => Ok(agent.chat(prompt, history).await?),
        }
    }

    /// Streams a multi-turn chat, returning a provider-agnostic [`ChatStream`].
    pub async fn stream_chat(&self, prompt: Message, history: Vec<Message>) -> ChatStream {
        match &self.inner {
            ConfiguredAgentKind::Ollama(agent) => {
                into_chat_stream(agent.stream_chat(prompt, history).multi_turn(5).await)
            }
            ConfiguredAgentKind::Copilot(agent) => {
                into_chat_stream(agent.stream_chat(prompt, history).await)
            }
            ConfiguredAgentKind::OpenAiCompletions(agent) => {
                into_chat_stream(agent.stream_chat(prompt, history).await)
            }
            ConfiguredAgentKind::OpenAiResponses(agent) => {
                into_chat_stream(agent.stream_chat(prompt, history).await)
            }
        }
    }
}

/// Wraps a [`StreamingResult`] into a [`ChatStream`] by erasing provider-specific payloads.
fn into_chat_stream<R: 'static>(stream: StreamingResult<R>) -> ChatStream {
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
