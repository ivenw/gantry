use anyhow::Result;
use futures::{Stream, StreamExt};
use rig::agent::{Agent, MultiTurnStreamItem, StreamingError};
use rig::client::{CompletionClient, Nothing};
use rig::completion::Chat;
use rig::message::Message;
use rig::providers::ollama;
use rig::streaming::{StreamedAssistantContent, StreamingChat};
use std::pin::Pin;

use crate::provider::{ModelSelection, ProviderConfig, ProviderConfigCatalog, ProviderId};

/// Provider-agnostic stream item. The `Final` variant inside [`StreamedAssistantContent`] carries
/// `()` because the raw provider chunk is not useful to callers; the assembled result is in
/// [`MultiTurnStreamItem::FinalResponse`].
pub type ChatStreamItem = MultiTurnStreamItem<()>;
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatStreamItem, StreamingError>> + Send>>;

#[derive(Clone)]
pub struct RigAgentFactory {
    catalog: ProviderConfigCatalog,
}

impl RigAgentFactory {
    /// Creates a new factory, validating the catalog before returning.
    pub fn new(catalog: ProviderConfigCatalog) -> Result<Self> {
        catalog.validate()?;
        Ok(Self { catalog })
    }

    /// Returns all configured providers.
    pub fn providers(&self) -> Vec<ProviderConfig> {
        self.catalog.providers.clone()
    }

    /// Returns the default [`ModelSelection`] from the catalog.
    pub fn default_selection(&self) -> Result<ModelSelection> {
        self.catalog.default_selection()
    }

    /// Returns the default [`ModelSelection`] for the given provider.
    pub fn default_selection_for(&self, provider_id: &ProviderId) -> Result<ModelSelection> {
        let model_id = self.catalog.provider_default_model(provider_id)?.clone();
        Ok(ModelSelection {
            provider_id: provider_id.clone(),
            model_id,
        })
    }

    /// Validates that the given [`ModelSelection`] refers to a configured provider and model.
    pub fn validate_selection(&self, selection: &ModelSelection) -> Result<()> {
        self.catalog.selection(selection)
    }

/// Builds a [`ConfiguredAgent`] for the given model selection and optional preamble.
    pub fn agent(
        &self,
        selection: &ModelSelection,
        preamble: Option<&str>,
    ) -> Result<ConfiguredAgent> {
        match self.provider_config(selection)? {
            ProviderConfig::Ollama(provider) => {
                let model = self
                    .catalog
                    .model(&selection.provider_id, &selection.model_id)
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "configured model '{}' not found for provider '{}'",
                            selection.model_id.as_str(),
                            selection.provider_id.as_str()
                        )
                    })?;

                let client = ollama::Client::builder()
                    .api_key(Nothing)
                    .base_url(&provider.base_url)
                    .build()?;

                let mut builder = client.agent(&model.provider_model_name);
                if let Some(p) = preamble {
                    builder = builder.preamble(p);
                }
                Ok(ConfiguredAgent::ollama(builder.build()))
            }
        }
    }

    fn provider_config(&self, selection: &ModelSelection) -> Result<ProviderConfig> {
        self.catalog
            .provider(&selection.provider_id)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "configured provider '{}' not found",
                    selection.provider_id.as_str()
                )
            })
    }
}

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

pub struct ConfiguredAgent {
    inner: ConfiguredAgentKind,
}

enum ConfiguredAgentKind {
    Ollama(Agent<ollama::CompletionModel>),
}

impl ConfiguredAgent {
    fn ollama(agent: Agent<ollama::CompletionModel>) -> Self {
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
