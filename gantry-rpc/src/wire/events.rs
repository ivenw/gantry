use gantry_core::{
    AppEvent, ErrorEvent, InitEvent, MessageReceivedEvent, PendingClearedEvent, StreamEndEvent,
    StreamStartEvent, TokenEvent,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum WireAppEvent {
    Init(InitEvent),
    MessageReceived(MessageReceivedEvent),
    StreamStart(StreamStartEvent),
    Token(TokenEvent),
    StreamEnd(StreamEndEvent),
    PendingCleared(PendingClearedEvent),
    Error(ErrorEvent),
}

impl From<&AppEvent> for WireAppEvent {
    fn from(ev: &AppEvent) -> Self {
        match ev {
            AppEvent::Init(e) => WireAppEvent::Init(e.clone()),
            AppEvent::MessageReceived(e) => WireAppEvent::MessageReceived(e.clone()),
            AppEvent::StreamStart(e) => WireAppEvent::StreamStart(e.clone()),
            AppEvent::Token(e) => WireAppEvent::Token(e.clone()),
            AppEvent::StreamEnd(e) => WireAppEvent::StreamEnd(e.clone()),
            AppEvent::PendingCleared(e) => WireAppEvent::PendingCleared(e.clone()),
            AppEvent::Error(e) => WireAppEvent::Error(e.clone()),
        }
    }
}

impl From<WireAppEvent> for AppEvent {
    fn from(ev: WireAppEvent) -> Self {
        match ev {
            WireAppEvent::Init(e) => AppEvent::Init(e),
            WireAppEvent::MessageReceived(e) => AppEvent::MessageReceived(e),
            WireAppEvent::StreamStart(e) => AppEvent::StreamStart(e),
            WireAppEvent::Token(e) => AppEvent::Token(e),
            WireAppEvent::StreamEnd(e) => AppEvent::StreamEnd(e),
            WireAppEvent::PendingCleared(e) => AppEvent::PendingCleared(e),
            WireAppEvent::Error(e) => AppEvent::Error(e),
        }
    }
}

