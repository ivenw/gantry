use gantry_core::{AppEvent, SessionId, SessionTree};

use crate::model::ChatMessage;

pub enum Msg {
    // Input
    Key(crossterm::event::KeyEvent),

    // Server app events
    AppEvent(AppEvent),

    // Streaming result
    StreamResult(Result<(), String>),

    // Command results
    SetStatus(String),
    NewSession(SessionId),

    // Scroll the chat window (positive = up, negative = down)
    ScrollChat(i32),

    // Tree view
    OpenTreeView(SessionTree),
    BranchTo(String),
    BranchToWithInput {
        branch_id: String,
        input: String,
    },
    ReloadMessages(Vec<ChatMessage>),
    ReloadMessagesWithInput(Vec<ChatMessage>, String),

    // Side-effect signals intercepted by Runtime before update()
    SendMessage(String),
    InterruptStream,
    ExecuteCommand(std::sync::Arc<dyn crate::commands::Command>),
    Quit,
}
