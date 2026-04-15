use anyhow::Result;
use futures::StreamExt;
use rig::agent::Agent;
use rig::client::{CompletionClient, Nothing};
use rig::completion::Chat;
use rig::message::Message;
use rig::providers::ollama;
use rig::streaming::StreamingChat;
use rig::{agent::MultiTurnStreamItem, streaming::StreamedAssistantContent};
use tokio::sync::mpsc;

use crate::{ModelSelection, ProviderConfig, ProviderConfigCatalog};

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
    // TODO: We actually wanted to return a AgentBuilder here. Since that has to be wrapped, we
    // probably still need the ConfiguredAgent wrapper too.
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

    // TODO: Skeptical if this warrants it's own helper
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

    // TODO: Change this interface to match rigs 1:1
    pub async fn chat(&self, prompt: Message, history: Vec<Message>) -> Result<String> {
        match &self.inner {
            ConfiguredAgentKind::Ollama(agent) => Ok(agent.chat(prompt, history).await?),
        }
    }

    // TODO: Change this interface to match rigs 1:1
    pub async fn stream_chat(
        &self,
        prompt: Message,
        history: Vec<Message>,
        token_tx: mpsc::Sender<String>,
    ) -> Result<()> {
        match &self.inner {
            ConfiguredAgentKind::Ollama(agent) => {
                let mut stream = agent.stream_chat(prompt, history).await;
                let mut token_count = 0usize;

                while let Some(next) = stream.next().await {
                    match next {
                        Ok(MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::Text(text),
                        )) => {
                            token_tx
                                .send(text.text)
                                .await
                                .map_err(|_| anyhow::anyhow!("token channel closed"))?;
                            token_count += 1;
                        }
                        Ok(_) => {}
                        Err(err) => {
                            return Err(anyhow::anyhow!("completion stream error: {}", err));
                        }
                    }
                }

                if token_count == 0 {
                    return Err(anyhow::anyhow!("stream returned zero tokens"));
                }

                Ok(())
            }
        }
    }
}
