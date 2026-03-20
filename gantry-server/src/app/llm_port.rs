use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use gantry_contract::{Message, Role};
use llm::builder::{LLMBackend, LLMBuilder};
use std::sync::Arc;
use tokio::sync::Mutex;

const OLLAMA_URL: &str = "http://localhost:11434";
const MODEL_NAME: &str = "qwen3.5";

#[async_trait]
pub trait LlmPort: Send + Sync {
    async fn generate_response(&self, messages: Vec<Message>) -> Result<String>;
    async fn generate_tokens(&self, messages: Vec<Message>) -> Result<Vec<String>>;
}

#[derive(Clone)]
pub struct OllamaLlmAdapter {
    provider: Arc<Mutex<Box<dyn llm::LLMProvider>>>,
}

impl OllamaLlmAdapter {
    pub async fn new() -> Result<Self> {
        let provider = LLMBuilder::new()
            .backend(LLMBackend::Ollama)
            .base_url(OLLAMA_URL)
            .model(MODEL_NAME)
            .reasoning(false)
            .build()?;

        Ok(Self {
            provider: Arc::new(Mutex::new(provider)),
        })
    }

    fn to_chat_messages(messages: Vec<Message>) -> Vec<llm::chat::ChatMessage> {
        messages
            .into_iter()
            .map(|msg| match msg.role {
                Role::User => llm::chat::ChatMessage::user().content(msg.content).build(),
                Role::Assistant => llm::chat::ChatMessage::assistant()
                    .content(msg.content)
                    .build(),
                Role::Error => llm::chat::ChatMessage::user()
                    .content(format!("[Error]: {}", msg.content))
                    .build(),
            })
            .collect()
    }
}

#[async_trait]
impl LlmPort for OllamaLlmAdapter {
    async fn generate_response(&self, messages: Vec<Message>) -> Result<String> {
        let llm_messages = Self::to_chat_messages(messages);
        let provider = self.provider.lock().await;
        let response = provider.chat(&llm_messages).await?;
        Ok(response.text().unwrap_or_default().to_string())
    }

    async fn generate_tokens(&self, messages: Vec<Message>) -> Result<Vec<String>> {
        let llm_messages = Self::to_chat_messages(messages);
        let provider = self.provider.lock().await;
        let mut stream = provider.chat_stream(&llm_messages).await?;

        let mut tokens = Vec::new();
        while let Some(next) = stream.next().await {
            match next {
                Ok(token) => tokens.push(token),
                Err(err) => {
                    tokens.push(format!("[Error]: {}", err));
                    break;
                }
            }
        }

        Ok(tokens)
    }
}
