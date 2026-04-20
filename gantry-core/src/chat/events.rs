#[derive(Debug, Clone)]
pub struct InitEvent {
    pub client_id: String,
    pub messages: Vec<super::Message>,
    pub pending_message: Option<super::PendingMessage>,
}

#[derive(Debug, Clone)]
pub struct MessageReceivedEvent {
    pub id: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct StreamStartEvent {
    pub message_id: String,
    pub pending_of: String,
}

#[derive(Debug, Clone)]
pub struct TokenEvent {
    pub message_id: String,
    pub delta: String,
}

#[derive(Debug, Clone)]
pub struct StreamEndEvent {
    pub message_id: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct PendingClearedEvent {
    pub pending_id: String,
}

#[derive(Debug, Clone)]
pub struct ErrorEvent {
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    Init(InitEvent),
    MessageReceived(MessageReceivedEvent),
    StreamStart(StreamStartEvent),
    Token(TokenEvent),
    StreamEnd(StreamEndEvent),
    PendingCleared(PendingClearedEvent),
    Error(ErrorEvent),
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMessageRequest {
    pub content: String,
}
