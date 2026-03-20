use super::event_bus::EventBus;
use super::llm_port::LlmPort;
use super::state::ConversationState;
use anyhow::Result;
use gantry_contract::{
    AppEvent, ErrorEvent, FormHiddenEvent, FormShownEvent, InitEvent, Message, MessageReceivedEvent,
    PendingClearedEvent, PendingMessage, Role, SelectFormResponse, StreamEndEvent, StreamMessageRequest,
    StreamStartEvent, TokenEvent,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppService {
    state: Arc<Mutex<ConversationState>>,
    event_bus: EventBus,
    llm: Arc<dyn LlmPort>,
    is_streaming: Arc<AtomicBool>,
}

impl AppService {
    pub fn new(llm: Arc<dyn LlmPort>) -> Self {
        Self {
            state: Arc::new(Mutex::new(ConversationState::default())),
            event_bus: EventBus::new(1000),
            llm,
            is_streaming: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<AppEvent> {
        self.event_bus.subscribe()
    }

    pub fn is_streaming(&self) -> bool {
        self.is_streaming.load(Ordering::SeqCst)
    }

    pub async fn init_event(&self, client_id: String) -> AppEvent {
        let state = self.state.lock().await;
        AppEvent::Init(InitEvent {
            client_id,
            messages: state.messages.clone(),
            pending_message: state.pending_message.clone(),
            form: state.active_form.clone(),
        })
    }

    pub async fn get_messages(&self) -> Vec<Message> {
        self.state.lock().await.messages.clone()
    }

    pub async fn clear_messages(&self) {
        self.state.lock().await.messages.clear();
    }

    pub async fn send_message(&self, content: String) -> Vec<Message> {
        {
            let mut state = self.state.lock().await;
            state.messages.push(Message::new(Role::User, content));
        }

        let snapshot = self.get_messages().await;
        let response = match self.llm.generate_response(snapshot).await {
            Ok(content) => Message::new(Role::Assistant, content),
            Err(err) => Message::new(Role::Error, err.to_string()),
        };

        let mut state = self.state.lock().await;
        state.messages.push(response);
        state.messages.clone()
    }

    pub async fn stream_message(&self, req: StreamMessageRequest) -> Result<PendingMessage> {
        if self
            .is_streaming
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(anyhow::anyhow!("a stream is already in progress"));
        }
        let _streaming_guard = StreamingGuard {
            is_streaming: self.is_streaming.clone(),
        };

        let pending = PendingMessage::new(req.content.clone());

        {
            let mut state = self.state.lock().await;
            state.messages.push(Message::new(Role::User, req.content));
            state.pending_message = Some(pending.clone());
        }

        self.event_bus
            .publish(AppEvent::MessageReceived(MessageReceivedEvent {
                id: pending.id.clone(),
                content: pending.content.clone(),
            }));

        let snapshot = self.get_messages().await;
        let tokens = match self.llm.generate_tokens(snapshot).await {
            Ok(tokens) => tokens,
            Err(err) => {
                self.clear_pending(&pending.id).await;
                self.event_bus.publish(AppEvent::Error(ErrorEvent {
                    message: err.to_string(),
                }));
                return Ok(pending);
            }
        };

        let message_id = Uuid::new_v4().to_string();
        self.event_bus
            .publish(AppEvent::StreamStart(StreamStartEvent {
                message_id: message_id.clone(),
                pending_of: pending.id.clone(),
            }));

        let mut accumulated = String::new();
        for token in tokens {
            accumulated.push_str(&token);
            self.event_bus.publish(AppEvent::Token(TokenEvent {
                message_id: message_id.clone(),
                delta: token,
            }));
        }

        self.event_bus
            .publish(AppEvent::StreamEnd(StreamEndEvent {
                message_id,
                content: accumulated.clone(),
            }));

        {
            let mut state = self.state.lock().await;
            state.messages.push(Message::new(Role::Assistant, accumulated));
        }

        self.clear_pending(&pending.id).await;
        Ok(pending)
    }

    pub async fn select_form(&self, form_id: String, selection: String) -> SelectFormResponse {
        let maybe_form = { self.state.lock().await.active_form.clone() };

        if let Some(form) = maybe_form {
            if form.id == form_id {
                self.hide_form(form.id.clone(), "client".to_string(), selection.clone())
                    .await;
                return SelectFormResponse {
                    success: true,
                    selected_by: Some("client".to_string()),
                    message: Some(format!("Selected: {}", selection)),
                };
            }

            return SelectFormResponse {
                success: false,
                selected_by: None,
                message: Some("Form not found".to_string()),
            };
        }

        SelectFormResponse {
            success: false,
            selected_by: None,
            message: Some("No active form".to_string()),
        }
    }

    pub async fn show_form(&self, options: Vec<String>) {
        let form = gantry_contract::FormState::new(options);
        {
            let mut state = self.state.lock().await;
            state.active_form = Some(form.clone());
        }
        self.event_bus.publish(AppEvent::FormShown(FormShownEvent {
            id: form.id,
            options: form.options,
        }));
    }

    pub async fn hide_form(&self, form_id: String, selected_by: String, selected: String) {
        {
            let mut state = self.state.lock().await;
            state.active_form = None;
        }

        self.event_bus
            .publish(AppEvent::FormHidden(FormHiddenEvent {
                id: form_id,
                selected_by,
                selected,
            }));
    }

    async fn clear_pending(&self, pending_id: &str) {
        {
            let mut state = self.state.lock().await;
            state.pending_message = None;
        }

        self.event_bus
            .publish(AppEvent::PendingCleared(PendingClearedEvent {
                pending_id: pending_id.to_string(),
            }));
    }
}

struct StreamingGuard {
    is_streaming: Arc<AtomicBool>,
}

impl Drop for StreamingGuard {
    fn drop(&mut self) {
        self.is_streaming.store(false, Ordering::SeqCst);
    }
}
