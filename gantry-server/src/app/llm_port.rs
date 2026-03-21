use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use gantry_contract::{Message, Role};
use rig::client::{CompletionClient, Nothing};
use rig::completion::Chat;
use rig::message::Message as RigMessage;
use rig::providers::ollama;
use rig::streaming::StreamingChat;
use rig::{agent::MultiTurnStreamItem, streaming::StreamedAssistantContent};
use tokio::sync::mpsc;

const OLLAMA_URL: &str = "http://localhost:11434";
const MODEL_NAME: &str = "ministral-3:3b";

#[async_trait]
pub trait LlmPort: Send + Sync {
    async fn generate_response(&self, messages: Vec<Message>) -> Result<String>;
    async fn generate_tokens(
        &self,
        messages: Vec<Message>,
        token_tx: mpsc::Sender<String>,
    ) -> Result<()>;
}

#[derive(Clone)]
pub struct OllamaLlmAdapter {
    client: ollama::Client,
    model_name: String,
}

impl OllamaLlmAdapter {
    pub async fn new() -> Result<Self> {
        let ollama_url =
            std::env::var("GANTRY_OLLAMA_URL").unwrap_or_else(|_| OLLAMA_URL.to_string());
        let model_name =
            std::env::var("GANTRY_OLLAMA_MODEL").unwrap_or_else(|_| MODEL_NAME.to_string());
        dbg!("llm.ollama.new", &ollama_url, &model_name);
        let client = ollama::Client::builder()
            .api_key(Nothing)
            .base_url(&ollama_url)
            .build()?;
        dbg!("llm.ollama.new.ready", &ollama_url, &model_name);

        Ok(Self { client, model_name })
    }

    fn to_rig_messages(messages: Vec<Message>) -> Vec<RigMessage> {
        messages
            .into_iter()
            .map(|msg| match msg.role {
                Role::User => RigMessage::user(msg.content),
                Role::Assistant => RigMessage::assistant(msg.content),
                Role::Error => RigMessage::user(format!("[Error]: {}", msg.content)),
            })
            .collect()
    }
}

#[async_trait]
impl LlmPort for OllamaLlmAdapter {
    async fn generate_response(&self, messages: Vec<Message>) -> Result<String> {
        dbg!("llm.generate_response.request_count", messages.len());
        let mut rig_messages = Self::to_rig_messages(messages);
        dbg!(
            "llm.generate_response.chat_messages_count",
            rig_messages.len()
        );
        let prompt = rig_messages.pop().ok_or_else(|| {
            anyhow::anyhow!("cannot generate response with empty message history")
        })?;
        dbg!("llm.generate_response.call_start");
        let agent = self.client.agent(&self.model_name).build();
        let text = agent.chat(prompt, rig_messages).await?;
        dbg!("llm.generate_response.call_done_len", text.len());
        Ok(text)
    }

    async fn generate_tokens(
        &self,
        messages: Vec<Message>,
        token_tx: mpsc::Sender<String>,
    ) -> Result<()> {
        dbg!("llm.generate_tokens.request_count", messages.len());
        let mut rig_messages = Self::to_rig_messages(messages);
        dbg!(
            "llm.generate_tokens.chat_messages_count",
            rig_messages.len()
        );
        let prompt = rig_messages
            .pop()
            .ok_or_else(|| anyhow::anyhow!("cannot generate tokens with empty message history"))?;
        dbg!("llm.generate_tokens.stream_start");
        let agent = self.client.agent(&self.model_name).build();
        let mut stream = agent.stream_chat(prompt, rig_messages).await;
        dbg!("llm.generate_tokens.stream_opened");

        let mut token_count = 0usize;
        while let Some(next) = stream.next().await {
            match next {
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(
                    text,
                ))) => {
                    let token = text.text;
                    dbg!("llm.generate_tokens.token", &token);
                    token_tx
                        .send(token)
                        .await
                        .map_err(|_| anyhow::anyhow!("token channel closed"))?;
                    token_count += 1;
                }
                Ok(_) => {}
                Err(err) => {
                    dbg!("llm.generate_tokens.stream_error", err.to_string());
                    return Err(anyhow::anyhow!("llm stream error: {}", err));
                }
            }
        }

        if token_count == 0 {
            return Err(anyhow::anyhow!("ollama stream returned zero tokens"));
        }

        dbg!("llm.generate_tokens.done_count", token_count);
        Ok(())
    }
}
