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

use crate::provider::{ModelSelection, ProviderConfig, ProviderConfigCatalog};

/// Events emitted by [`ConfiguredAgent::stream_chat`] to the caller.
#[derive(Debug)]
pub enum AgentStreamEvent {
    /// A text delta from the assistant.
    Token(String),
    /// A tool call has started; the tool is now executing.
    ToolCallStarted {
        tool_call_id: String,
        tool_name: String,
    },
    /// A tool result is available.
    ToolResultReceived {
        tool_call_id: String,
        tool_name: String,
        content: String,
    },
}

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

    /// Streams a multi-turn chat, forwarding events to `event_tx`.
    ///
    /// Text deltas, tool-call starts, and tool results are all emitted as [`AgentStreamEvent`]s.
    /// Returns an error if the stream yields zero tokens.
    pub async fn stream_chat(
        &self,
        prompt: Message,
        history: Vec<Message>,
        event_tx: mpsc::Sender<AgentStreamEvent>,
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
                            event_tx
                                .send(AgentStreamEvent::Token(text.text))
                                .await
                                .map_err(|_| anyhow::anyhow!("event channel closed"))?;
                            token_count += 1;
                        }
                        Ok(MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::ToolCall {
                                tool_call,
                                internal_call_id: _,
                            },
                        )) => {
                            event_tx
                                .send(AgentStreamEvent::ToolCallStarted {
                                    tool_call_id: tool_call.id.clone(),
                                    tool_name: tool_call.function.name.clone(),
                                })
                                .await
                                .map_err(|_| anyhow::anyhow!("event channel closed"))?;
                        }
                        Ok(MultiTurnStreamItem::StreamUserItem(
                            rig::streaming::StreamedUserContent::ToolResult { tool_result, .. },
                        )) => {
                            let content = tool_result
                                .content
                                .iter()
                                .map(|c| match c {
                                    rig::message::ToolResultContent::Text(t) => t.text.clone(),
                                    rig::message::ToolResultContent::Image(_) => String::new(),
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            let tool_call_id = tool_result
                                .call_id
                                .clone()
                                .unwrap_or_else(|| tool_result.id.clone());
                            event_tx
                                .send(AgentStreamEvent::ToolResultReceived {
                                    tool_call_id,
                                    tool_name: tool_result.id.clone(),
                                    content,
                                })
                                .await
                                .map_err(|_| anyhow::anyhow!("event channel closed"))?;
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
