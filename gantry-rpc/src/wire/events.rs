use gantry_core::{AppEvent, Message, PendingMessage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireInitEvent {
    pub client_id: String,
    pub messages: Vec<Message>,
    pub pending_message: Option<PendingMessage>,
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
    Error(WireErrorEvent),
}

impl From<&AppEvent> for WireAppEvent {
    fn from(ev: &AppEvent) -> Self {
        match ev {
            AppEvent::Init(e) => WireAppEvent::Init(WireInitEvent {
                client_id: e.client_id.clone(),
                messages: e.messages.clone(),
                pending_message: e.pending_message.clone(),
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
                messages: e.messages,
                pending_message: e.pending_message,
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
            WireAppEvent::Error(e) => {
                AppEvent::Error(gantry_core::ErrorEvent { message: e.message })
            }
        }
    }
}
