use gantry_core::{ChatStreamItem, ModelSelection, ProviderAlias, ProviderConfig, SessionId, SessionInfo, SessionTree, StoredCredential, StreamingError, Usage};

use crate::model::ChatMessage;

pub enum Msg {
    // Input
    Key(crossterm::event::KeyEvent),

    // Stream events from the agent
    StreamItem(Result<ChatStreamItem, StreamingError>),
    StreamDone,
    ToolCallStarted { name: String, id: String },
    ToolCallFinished { id: String },

    // Streaming result
    StreamResult(Result<(), String>),

    // Command results
    SetStatus(String),
    NewSession,

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

    // Sessions browser
    OpenSessionsView(Vec<SessionInfo>, SessionId),
    ResumeSession(SessionId),

    ModelSelectionChanged(Option<ModelSelection>),
    UsageUpdated(Usage),

    // Providers overlay
    OpenProvidersView(Vec<ProviderConfig>),
    AddProvider(ProviderConfig, Option<StoredCredential>),
    RemoveProvider(ProviderAlias),

    // Model picker overlay
    OpenModelPicker(Vec<ModelSelection>),
    SelectModel(ModelSelection),

    // Side-effect signals intercepted by Runtime before update()
    SendMessage(String),
    InterruptStream,
    ExecuteCommand(std::sync::Arc<dyn crate::commands::Command>),
    Quit,
}
