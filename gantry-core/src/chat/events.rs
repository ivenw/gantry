use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitEvent {
    pub client_id: String,
    pub messages: Vec<super::Message>,
    pub pending_message: Option<super::PendingMessage>,
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
            AppEvent::Error(_) => 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMessageRequest {
    pub content: String,
}

fn parse_id(s: &str) -> u64 {
    s.chars()
        .filter(|c| c.is_ascii_digit())
        .take(18)
        .collect::<String>()
        .parse()
        .unwrap_or(0)
}
