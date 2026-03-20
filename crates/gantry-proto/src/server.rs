use anyhow::Result;
use futures::StreamExt;
use gantry_types::{Message, Role};
use llm::error::LLMError;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::events::{FormState, PendingMessage};
use crate::llm::LlmClient;
use crate::sse_server::ClientRegistry;

const TOKEN_BATCH_SIZE: usize = 5;

#[derive(Clone)]
pub struct JsonRpcServer {
    messages: Arc<Mutex<Vec<Message>>>,
    token_sender: broadcast::Sender<TokenUpdate>,
    llm_client: Arc<Mutex<Option<LlmClient>>>,
    is_streaming: Arc<AtomicBool>,
    client_registry: Arc<ClientRegistry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenUpdate {
    pub batch: String,
    pub is_complete: bool,
}

impl JsonRpcServer {
    pub fn new(registry: Arc<ClientRegistry>) -> Self {
        let (sender, _) = broadcast::channel(100);
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            token_sender: sender,
            llm_client: Arc::new(Mutex::new(None)),
            is_streaming: Arc::new(AtomicBool::new(false)),
            client_registry: registry,
        }
    }

    pub async fn init_llm_client(&self) -> Result<()> {
        let client = LlmClient::new().await?;
        let mut llm_client = self.llm_client.lock().unwrap();
        *llm_client = Some(client);
        Ok(())
    }

    pub fn is_streaming(&self) -> bool {
        self.is_streaming.load(Ordering::SeqCst)
    }

    pub async fn send_message(&self, content: String) -> Vec<Message> {
        let user_msg = Message::new(Role::User, content);
        self.messages.lock().unwrap().push(user_msg);

        let response = match self.get_llm_response().await {
            Ok(llm_response) => llm_response,
            Err(e) => Message::new(Role::Error, e.to_string()),
        };

        self.messages.lock().unwrap().push(response.clone());
        let _ = self.token_sender.send(TokenUpdate {
            batch: response.content.clone(),
            is_complete: true,
        });
        self.messages.lock().unwrap().clone()
    }

    pub async fn stream_message(&self, content: String) -> Vec<Message> {
        self.is_streaming.store(true, Ordering::SeqCst);

        let user_msg = Message::new(Role::User, content);
        self.messages.lock().unwrap().push(user_msg);

        let llm_client = match {
            let guard = self.llm_client.lock().unwrap();
            guard.clone()
        } {
            Some(client) => client,
            None => {
                let error_msg = Message::new(Role::Error, "LLM client not initialized".to_string());
                self.messages.lock().unwrap().push(error_msg);
                self.is_streaming.store(false, Ordering::SeqCst);
                return self.messages.lock().unwrap().clone();
            }
        };

        let messages = self.get_messages();
        let stream_result = llm_client.generate_streaming_response(messages, TOKEN_BATCH_SIZE).await;

        let mut stream: Pin<Box<dyn futures::Stream<Item = Result<String, LLMError>> + Send + '_>> = match stream_result {
            Ok(s) => s,
            Err(e) => {
                let error_msg = Message::new(Role::Error, e.to_string());
                self.messages.lock().unwrap().push(error_msg);
                self.is_streaming.store(false, Ordering::SeqCst);
                return self.messages.lock().unwrap().clone();
            }
        };

        let mut accumulated = String::new();

        while let Some(batch_result) = stream.next().await {
            match batch_result {
                Ok(batch) => {
                    accumulated.push_str(&batch);
                    let _ = self.token_sender.send(TokenUpdate {
                        batch: accumulated.clone(),
                        is_complete: false,
                    });
                }
                Err(e) => {
                    accumulated.push_str(&format!("[Error]: {}", e));
                    let _ = self.token_sender.send(TokenUpdate {
                        batch: accumulated.clone(),
                        is_complete: true,
                    });
                    break;
                }
            }
        }

        let assistant_msg = Message::new(Role::Assistant, accumulated.clone());
        self.messages.lock().unwrap().push(assistant_msg);

        let _ = self.token_sender.send(TokenUpdate {
            batch: accumulated,
            is_complete: true,
        });

        self.is_streaming.store(false, Ordering::SeqCst);
        self.messages.lock().unwrap().clone()
    }

    async fn get_llm_response(&self) -> Result<Message> {
        let llm_client = {
            let guard = self.llm_client.lock().unwrap();
            guard.clone().ok_or_else(|| anyhow::anyhow!("LLM client not initialized"))?
        };
        let messages = self.get_messages();
        let response = llm_client.generate_response(messages).await?;
        Ok(response)
    }

    pub fn get_messages(&self) -> Vec<Message> {
        self.messages.lock().unwrap().clone()
    }

    pub fn clear_messages(&self) {
        self.messages.lock().unwrap().clear();
    }

    pub fn subscribe_to_tokens(&self) -> broadcast::Receiver<TokenUpdate> {
        self.token_sender.subscribe()
    }
}

impl Default for JsonRpcServer {
    fn default() -> Self {
        let registry = Arc::new(ClientRegistry::new());
        Self::new(registry)
    }
}

impl JsonRpcServer {
    pub async fn stream_message_sse(
        &self,
        client_id: String,
        content: String,
    ) -> Result<PendingMessage> {
        self.is_streaming.store(true, Ordering::SeqCst);

        let pending = PendingMessage {
            id: Uuid::new_v4().to_string(),
            client_id: client_id.clone(),
            content: content.clone(),
        };

        let user_msg = Message::new(Role::User, content);
        self.messages.lock().unwrap().push(user_msg);

        self.client_registry.broadcast_message_received(&pending);
        self.client_registry.set_pending_message(Some(pending.clone()));

        let llm_client = match {
            let guard = self.llm_client.lock().unwrap();
            guard.clone()
        } {
            Some(client) => client,
            None => {
                self.client_registry.broadcast_pending_cleared(&pending.id);
                self.client_registry.set_pending_message(None);
                self.client_registry.broadcast_error("LLM client not initialized");
                self.is_streaming.store(false, Ordering::SeqCst);
                return Ok(pending);
            }
        };

        let messages = self.get_messages();
        let stream_result = llm_client.generate_token_stream(messages).await;

        let mut stream: Pin<Box<dyn futures::Stream<Item = Result<String, LLMError>> + Send + '_>> = match stream_result {
            Ok(s) => s,
            Err(e) => {
                self.client_registry.broadcast_pending_cleared(&pending.id);
                self.client_registry.set_pending_message(None);
                self.client_registry.broadcast_error(&e.to_string());
                self.is_streaming.store(false, Ordering::SeqCst);
                return Ok(pending);
            }
        };

        let message_id = Uuid::new_v4().to_string();
        self.client_registry.broadcast_stream_start(&message_id, &pending.id);

        let mut accumulated = String::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(token) => {
                    dbg!("Token: ", &token);
                    accumulated.push_str(&token);
                    self.client_registry.broadcast_token(&message_id, &token);
                }
                Err(e) => {
                    dbg!("Stream error: ", &e);
                    accumulated.push_str(&format!("[Error]: {}", e));
                    self.client_registry.broadcast_token(&message_id, &format!("[Error]: {}", e));
                    break;
                }
            }
        }

        self.client_registry.broadcast_stream_end(&message_id, &accumulated);
        self.client_registry.set_pending_message(None);

        dbg!("LLM Response: ", &accumulated);
        let assistant_msg = Message::new(Role::Assistant, accumulated);
        self.messages.lock().unwrap().push(assistant_msg);

        self.is_streaming.store(false, Ordering::SeqCst);
        Ok(pending)
    }

    pub fn get_pending_message(&self) -> Option<PendingMessage> {
        self.client_registry.get_pending_message()
    }

    pub fn get_active_form(&self) -> Option<FormState> {
        self.client_registry.get_active_form()
    }

    pub fn show_form(&self, form: FormState) {
        self.client_registry.broadcast_form_shown(&form);
    }

    pub fn hide_form(&self, form: &FormState, selected_by: &str, selected: &str) {
        self.client_registry.broadcast_form_hidden(form, selected_by, selected);
    }
}
