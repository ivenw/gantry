use anyhow::Result;
use futures::StreamExt;
use gantry_types::{Message, Role};
use llm::builder::{LLMBackend, LLMBuilder};
use llm::error::LLMError;
use std::pin::Pin;
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

    pub async fn generate_streaming_response(
        &self,
        messages: Vec<Message>,
        batch_size: usize,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<String, LLMError>> + Send + '_>>> {
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
        let stream_future = provider.chat_stream(&llm_messages);
        let stream = stream_future.await?;

        Ok(Box::pin(async_stream::stream! {
            let mut stream = stream;
            let mut buffer = String::new();
            let mut count = 0;

            while let Some(result) = stream.next().await {
                match result {
                    Ok(token) => {
                        buffer.push_str(&token);
                        count += 1;

                        if count >= batch_size {
                            yield Ok(buffer.clone());
                            buffer.clear();
                            count = 0;
                        }
                    }
                    Err(e) => {
                        yield Err(e);
                        break;
                    }
                }
            }

            if !buffer.is_empty() {
                yield Ok(buffer);
            }
        }))
    }

    pub async fn generate_token_stream(
        &self,
        messages: Vec<Message>,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<String, LLMError>> + Send + '_>>> {
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
        let stream_future = provider.chat_stream(&llm_messages);
        let stream = stream_future.await?;

        Ok(Box::pin(async_stream::stream! {
            let mut stream = stream;

            while let Some(result) = stream.next().await {
                match result {
                    Ok(token) => {
                        yield Ok(token);
                    }
                    Err(e) => {
                        yield Err(e);
                        break;
                    }
                }
            }
        }))
    }
}
