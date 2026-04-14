use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Error,
}

impl Role {
    pub fn label(&self) -> &'static str {
        match self {
            Role::User => "You",
            Role::Assistant => "Assistant",
            Role::Error => "Error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHeader {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    pub created_at: String,
}

impl SessionHeader {
    pub fn new(id: String) -> Self {
        Self {
            kind: "header".to_string(),
            id,
            created_at: now_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub parent_id: Option<String>,
    pub role: Role,
    pub content: String,
    pub created_at: String,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            parent_id: None,
            role,
            content: content.into(),
            created_at: now_rfc3339(),
        }
    }

    pub fn with_parent(mut self, parent_id: String) -> Self {
        self.parent_id = Some(parent_id);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingMessage {
    pub id: String,
    pub content: String,
}

impl PendingMessage {
    pub fn new(content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum AppEvent {
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

impl AppEvent {
    pub fn id(&self) -> u64 {
        match self {
            AppEvent::Init(e) => parse_id(&e.client_id),
            AppEvent::MessageReceived(e) => parse_id(&e.id),
            AppEvent::StreamStart(e) => parse_id(&e.message_id),
            AppEvent::Token(e) => parse_id(&e.message_id),
            AppEvent::StreamEnd(e) => parse_id(&e.message_id),
            AppEvent::PendingCleared(e) => parse_id(&e.pending_id),
            AppEvent::FormShown(e) => parse_id(&e.id),
            AppEvent::FormHidden(e) => parse_id(&e.id),
            AppEvent::Error(_) => 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMessageRequest {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectFormRequest {
    pub form_id: String,
    pub selection: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectFormResponse {
    pub success: bool,
    pub selected_by: Option<String>,
    pub message: Option<String>,
}

fn parse_id(s: &str) -> u64 {
    s.chars()
        .filter(|c| c.is_ascii_digit())
        .take(18)
        .collect::<String>()
        .parse()
        .unwrap_or(0)
}
