use gantry_types::Message;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingMessage {
    pub id: String,
    pub client_id: String,
    pub content: String,
}

impl PendingMessage {
    pub fn new(client_id: &ClientId, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            client_id: client_id.to_string(),
            content,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormState {
    pub id: String,
    pub options: Vec<String>,
}

impl FormState {
    pub fn new(options: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            options,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitEvent {
    pub client_id: String,
    pub messages: Vec<Message>,
    pub pending_message: Option<PendingMessage>,
    pub form: Option<FormState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageReceivedEvent {
    pub id: String,
    pub client_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamStartEvent {
    pub message_id: String,
    pub pending_of: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenEvent {
    pub message_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamEndEvent {
    pub message_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingClearedEvent {
    pub pending_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormShownEvent {
    pub id: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormHiddenEvent {
    pub id: String,
    pub selected_by: String,
    pub selected: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum SseEvent {
    Init(InitEvent),
    MessageReceived(MessageReceivedEvent),
    StreamStart(StreamStartEvent),
    Token(TokenEvent),
    StreamEnd(StreamEndEvent),
    PendingCleared(PendingClearedEvent),
    FormShown(FormShownEvent),
    FormHidden(FormHiddenEvent),
    Error(ErrorEvent),
}

impl SseEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            SseEvent::Init(_) => "init",
            SseEvent::MessageReceived(_) => "message_received",
            SseEvent::StreamStart(_) => "stream_start",
            SseEvent::Token(_) => "token",
            SseEvent::StreamEnd(_) => "stream_end",
            SseEvent::PendingCleared(_) => "pending_cleared",
            SseEvent::FormShown(_) => "form_shown",
            SseEvent::FormHidden(_) => "form_hidden",
            SseEvent::Error(_) => "error",
        }
    }

    pub fn id(&self) -> u64 {
        match self {
            SseEvent::Init(e) => parse_id(&e.client_id),
            SseEvent::MessageReceived(e) => parse_id(&e.id),
            SseEvent::StreamStart(e) => parse_id(&e.message_id),
            SseEvent::Token(e) => parse_id(&e.message_id),
            SseEvent::StreamEnd(e) => parse_id(&e.message_id),
            SseEvent::PendingCleared(e) => parse_id(&e.pending_id),
            SseEvent::FormShown(e) => parse_id(&e.id),
            SseEvent::FormHidden(e) => parse_id(&e.id),
            SseEvent::Error(_) => 0,
        }
    }

    pub fn to_sse_format(&self) -> String {
        let id = self.id();
        let event_type = self.event_type();
        let data = serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string());
        format!("id: {}\nevent: {}\ndata: {}\n\n", id, event_type, data)
    }
}

fn parse_id(s: &str) -> u64 {
    s.chars()
        .filter(|c| c.is_ascii_digit())
        .take(18)
        .collect::<String>()
        .parse()
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(pub Uuid);

impl ClientId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ClientId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<ClientId> for String {
    fn from(id: ClientId) -> Self {
        id.0.to_string()
    }
}

pub fn create_init_event(
    client_id: &ClientId,
    messages: Vec<Message>,
    pending_message: Option<PendingMessage>,
    form: Option<FormState>,
) -> SseEvent {
    SseEvent::Init(InitEvent {
        client_id: client_id.to_string(),
        messages,
        pending_message,
        form,
    })
}

pub fn create_message_received_event(pending: &PendingMessage) -> SseEvent {
    SseEvent::MessageReceived(MessageReceivedEvent {
        id: pending.id.clone(),
        client_id: pending.client_id.clone(),
        content: pending.content.clone(),
    })
}

pub fn create_stream_start_event(message_id: &str, pending_of: &str) -> SseEvent {
    SseEvent::StreamStart(StreamStartEvent {
        message_id: message_id.to_string(),
        pending_of: pending_of.to_string(),
    })
}

pub fn create_token_event(message_id: &str, delta: &str) -> SseEvent {
    SseEvent::Token(TokenEvent {
        message_id: message_id.to_string(),
        delta: delta.to_string(),
    })
}

pub fn create_stream_end_event(message_id: &str, content: &str) -> SseEvent {
    SseEvent::StreamEnd(StreamEndEvent {
        message_id: message_id.to_string(),
        content: content.to_string(),
    })
}

pub fn create_pending_cleared_event(pending_id: &str) -> SseEvent {
    SseEvent::PendingCleared(PendingClearedEvent {
        pending_id: pending_id.to_string(),
    })
}

pub fn create_form_shown_event(form: &FormState) -> SseEvent {
    SseEvent::FormShown(FormShownEvent {
        id: form.id.clone(),
        options: form.options.clone(),
    })
}

pub fn create_form_hidden_event(form: &FormState, selected_by: &str, selected: &str) -> SseEvent {
    SseEvent::FormHidden(FormHiddenEvent {
        id: form.id.clone(),
        selected_by: selected_by.to_string(),
        selected: selected.to_string(),
    })
}

pub fn create_error_event(message: &str) -> SseEvent {
    SseEvent::Error(ErrorEvent {
        message: message.to_string(),
    })
}
