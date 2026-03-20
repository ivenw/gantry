use anyhow::Result;
use gantry_types::{Message, Role};
use llm::builder::{LLMBackend, LLMBuilder};
use std::sync::Arc;
use tokio::sync::Mutex;

const OLLAMA_URL: &str = "http://localhost:11434";
const MODEL_NAME: &str = "qwen3.5";

#[derive(Clone)]
pub struct LlmClient {
    provider: Arc<Mutex<Box<dyn llm::LLMProvider>>>,
}

impl LlmClient {
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

    pub async fn generate_response(&self, messages: Vec<Message>) -> Result<Message> {
        let llm_messages: Vec<llm::chat::ChatMessage> = messages
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
            .collect();

        let provider = self.provider.lock().await;
        let response = provider.chat(&llm_messages).await?;
        let content = response.text().unwrap_or_default().to_string();

        Ok(Message::new(Role::Assistant, content))
    }
}
