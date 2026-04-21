use crate::chat::events::StreamMessageRequest;
use crate::chat::stream::{StreamEvent, stream_message_with_factory};
use crate::chat::system_prompt::build_system_prompt;
use crate::project::resource_loader::discover_agents_md;
use crate::provider::agent_factory::RigAgentFactory;
use crate::provider::{ModelId, ModelSelection, ProviderConfig, ProviderId};
use anyhow::Result;
use crate::session::SessionHandle;
use rig::message::Message;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// Composes model selection, system prompt, and message history into LLM requests.
pub struct ChatService {
    agent_factory: RigAgentFactory,
}

impl ChatService {
    /// Creates a new chat service backed by the given agent factory.
    pub fn new(agent_factory: RigAgentFactory) -> Self {
        Self { agent_factory }
    }

    /// Returns all configured providers with their available models.
    pub fn list_providers(&self) -> Vec<ProviderConfig> {
        self.agent_factory.catalog().providers.clone()
    }

    /// Validates and sets the active provider on the handle, using its default model.
    pub async fn set_active_provider(
        &self,
        handle: &SessionHandle,
        provider_id: ProviderId,
    ) -> Result<()> {
        let model_id = self
            .agent_factory
            .catalog()
            .provider_default_model(&provider_id)?
            .clone();
        self.set_active_model_selection(
            handle,
            ModelSelection {
                provider_id,
                model_id,
            },
        )
        .await
    }

    /// Validates and sets the active model on the handle, keeping the current provider.
    pub async fn set_active_model(&self, handle: &SessionHandle, model_id: ModelId) -> Result<()> {
        let provider_id = handle.get_active_selection().await.provider_id;
        self.set_active_model_selection(
            handle,
            ModelSelection {
                provider_id,
                model_id,
            },
        )
        .await
    }

    /// Validates the selection against the catalog and updates the handle.
    pub async fn set_active_model_selection(
        &self,
        handle: &SessionHandle,
        selection: ModelSelection,
    ) -> Result<()> {
        self.agent_factory.catalog().selection(&selection)?;
        handle.set_active_selection(selection).await;
        Ok(())
    }

    /// Sends a message and returns the updated message list, or an error if the LLM call fails.
    pub async fn send_message(
        &self,
        handle: Arc<SessionHandle>,
        content: String,
    ) -> Result<Vec<Message>> {
        dbg!("chat_service.send_message.request", &content);
        handle
            .append_message(Message::user(content))
            .await
            .unwrap_or_else(|_| panic!("failed to persist message"));

        let (mut rig_messages, selection) = handle.snapshot().await;
        let system_prompt = build_system_prompt(&discover_agents_md(&handle.project_path));
        dbg!("chat_service.send_message.snapshot_len", rig_messages.len());
        let result = match rig_messages.pop() {
            Some(prompt) => match self
                .agent_factory
                .agent(&selection, Some(&system_prompt))
                .await
            {
                Ok(agent) => match agent.chat(prompt, rig_messages).await {
                    Ok(content) => {
                        dbg!("chat_service.send_message.llm_ok_len", content.len());
                        Ok(content)
                    }
                    Err(err) => {
                        dbg!("chat_service.send_message.llm_err", err.to_string());
                        Err(err.to_string())
                    }
                },
                Err(err) => {
                    dbg!("chat_service.send_message.agent_err", err.to_string());
                    Err(err.to_string())
                }
            },
            None => Err("cannot generate response with empty message history".to_string()),
        };

        match result {
            Ok(content) => {
                handle.append_message(Message::assistant(content)).await?;
            }
            Err(err) => return Err(anyhow::anyhow!(err)),
        }

        let messages = handle.get_messages().await;
        dbg!(
            "chat_service.send_message.response_messages_len",
            messages.len()
        );
        Ok(messages)
    }

    /// Starts streaming a message and returns the pending message ID, a cancel sender, and a
    /// receiver of stream events. The caller drives the event receiver and handles cancellation
    /// via the cancel sender.
    pub async fn stream_message(
        &self,
        handle: Arc<SessionHandle>,
        req: StreamMessageRequest,
    ) -> Result<(String, oneshot::Sender<()>, mpsc::Receiver<StreamEvent>)> {
        stream_message_with_factory(req, handle, &self.agent_factory).await
    }
}
