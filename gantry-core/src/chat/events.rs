#[derive(Debug, Clone)]
pub struct InitEvent {
    pub client_id: String,
    pub messages: Vec<rig::message::Message>,
    pub pending_message: Option<rig::message::Message>,
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
pub struct ToolCallStartedEvent {
    pub tool_call_id: String,
    pub tool_name: String,
}

#[derive(Debug, Clone)]
pub struct ToolResultReceivedEvent {
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: String,
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
    ToolCallStarted(ToolCallStartedEvent),
    ToolResultReceived(ToolResultReceivedEvent),
    Error(ErrorEvent),
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMessageRequest {
    pub content: String,
}
