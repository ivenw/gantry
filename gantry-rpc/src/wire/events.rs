use gantry_core::AppEvent;
use rig::message::Message;
use serde::{Deserialize, Serialize};

use super::WireMessage;
use super::message::to_wire;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireInitEvent {
    pub client_id: String,
    pub messages: Vec<WireMessage>,
    pub pending_message: Option<WireMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireMessageReceivedEvent {
    pub id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireStreamStartEvent {
    pub message_id: String,
    pub pending_of: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireTokenEvent {
    pub message_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireStreamEndEvent {
    pub message_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WirePendingClearedEvent {
    pub pending_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireToolCallStartedEvent {
    pub tool_call_id: String,
    pub tool_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireToolResultReceivedEvent {
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireErrorEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum WireAppEvent {
    Init(WireInitEvent),
    MessageReceived(WireMessageReceivedEvent),
    StreamStart(WireStreamStartEvent),
    Token(WireTokenEvent),
    StreamEnd(WireStreamEndEvent),
    PendingCleared(WirePendingClearedEvent),
    ToolCallStarted(WireToolCallStartedEvent),
    ToolResultReceived(WireToolResultReceivedEvent),
    Error(WireErrorEvent),
}

impl From<&AppEvent> for WireAppEvent {
    fn from(ev: &AppEvent) -> Self {
        match ev {
            AppEvent::Init(e) => WireAppEvent::Init(WireInitEvent {
                client_id: e.client_id.clone(),
                messages: e.messages.iter().filter_map(to_wire).collect(),
                pending_message: e.pending_message.as_ref().and_then(to_wire),
            }),
            AppEvent::MessageReceived(e) => {
                WireAppEvent::MessageReceived(WireMessageReceivedEvent {
                    id: e.id.clone(),
                    content: e.content.clone(),
                })
            }
            AppEvent::StreamStart(e) => WireAppEvent::StreamStart(WireStreamStartEvent {
                message_id: e.message_id.clone(),
                pending_of: e.pending_of.clone(),
            }),
            AppEvent::Token(e) => WireAppEvent::Token(WireTokenEvent {
                message_id: e.message_id.clone(),
                delta: e.delta.clone(),
            }),
            AppEvent::StreamEnd(e) => WireAppEvent::StreamEnd(WireStreamEndEvent {
                message_id: e.message_id.clone(),
                content: e.content.clone(),
            }),
            AppEvent::PendingCleared(e) => WireAppEvent::PendingCleared(WirePendingClearedEvent {
                pending_id: e.pending_id.clone(),
            }),
            AppEvent::ToolCallStarted(e) => {
                WireAppEvent::ToolCallStarted(WireToolCallStartedEvent {
                    tool_call_id: e.tool_call_id.clone(),
                    tool_name: e.tool_name.clone(),
                })
            }
            AppEvent::ToolResultReceived(e) => {
                WireAppEvent::ToolResultReceived(WireToolResultReceivedEvent {
                    tool_call_id: e.tool_call_id.clone(),
                    tool_name: e.tool_name.clone(),
                    content: e.content.clone(),
                })
            }
            AppEvent::Error(e) => WireAppEvent::Error(WireErrorEvent {
                message: e.message.clone(),
            }),
        }
    }
}

impl From<WireAppEvent> for AppEvent {
    fn from(ev: WireAppEvent) -> Self {
        match ev {
            WireAppEvent::Init(e) => AppEvent::Init(gantry_core::InitEvent {
                client_id: e.client_id,
                messages: e.messages.into_iter().map(Message::from).collect(),
                pending_message: e.pending_message.map(Message::from),
            }),
            WireAppEvent::MessageReceived(e) => {
                AppEvent::MessageReceived(gantry_core::MessageReceivedEvent {
                    id: e.id,
                    content: e.content,
                })
            }
            WireAppEvent::StreamStart(e) => AppEvent::StreamStart(gantry_core::StreamStartEvent {
                message_id: e.message_id,
                pending_of: e.pending_of,
            }),
            WireAppEvent::Token(e) => AppEvent::Token(gantry_core::TokenEvent {
                message_id: e.message_id,
                delta: e.delta,
            }),
            WireAppEvent::StreamEnd(e) => AppEvent::StreamEnd(gantry_core::StreamEndEvent {
                message_id: e.message_id,
                content: e.content,
            }),
            WireAppEvent::PendingCleared(e) => {
                AppEvent::PendingCleared(gantry_core::PendingClearedEvent {
                    pending_id: e.pending_id,
                })
            }
            WireAppEvent::ToolCallStarted(e) => {
                AppEvent::ToolCallStarted(gantry_core::ToolCallStartedEvent {
                    tool_call_id: e.tool_call_id,
                    tool_name: e.tool_name,
                })
            }
            WireAppEvent::ToolResultReceived(e) => {
                AppEvent::ToolResultReceived(gantry_core::ToolResultReceivedEvent {
                    tool_call_id: e.tool_call_id,
                    tool_name: e.tool_name,
                    content: e.content,
                })
            }
            WireAppEvent::Error(e) => {
                AppEvent::Error(gantry_core::ErrorEvent { message: e.message })
            }
        }
    }
}
