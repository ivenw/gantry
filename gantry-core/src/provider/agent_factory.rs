use anyhow::Result;
use futures::Stream;
use rig::agent::{Agent, MultiTurnStreamItem, StreamingError};
use rig::client::{CompletionClient, Nothing};
use rig::completion::Chat;
use rig::message::Message;
use rig::providers::ollama;
use rig::streaming::StreamingChat;
use std::pin::Pin;

use crate::provider::{ModelSelection, ProviderConfig, ProviderConfigCatalog};

pub type ChatStreamItem = MultiTurnStreamItem<ollama::StreamingCompletionResponse>;
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatStreamItem, StreamingError>> + Send>>;

// TODO: Skeptical if we need this as a struct. A factory function seems better
#[derive(Clone)]
pub struct RigAgentFactory {
    catalog: ProviderConfigCatalog,
}

impl RigAgentFactory {
    pub fn new(catalog: ProviderConfigCatalog) -> Result<Self> {
        catalog.validate()?;
        Ok(Self { catalog })
    }

    // TODO: skeptical if this should be pub
    pub fn catalog(&self) -> &ProviderConfigCatalog {
        &self.catalog
    }

    // TODO: Why is this async? We are not awaiting anything inside of it.
    pub async fn agent(
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

    /// Streams a multi-turn chat, returning a stream of [`MultiTurnStreamItem`]s.
    pub async fn stream_chat(&self, prompt: Message, history: Vec<Message>) -> ChatStream {
        match &self.inner {
            ConfiguredAgentKind::Ollama(agent) => Box::pin(agent.stream_chat(prompt, history).await),
        }
    }
}
