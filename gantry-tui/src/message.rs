use gantry_core::{ChatStreamItem, ContextWindow, InputToken, ModelSelection, ProviderAlias, ProviderConfig, Usage, SessionId, SessionInfo, SessionTree, StoredCredential, StreamingError};

use crate::model::ChatMessage;

pub enum Msg {
    // Input
    Key(crossterm::event::KeyEvent),

    // Stream events from the agent
    StreamItem(Result<ChatStreamItem, StreamingError>),
    StreamDone,
    ToolCallStarted { name: String, id: String },
    ToolCallFinished { id: String },

    // Streaming error (stream task failed; StreamDone is not sent in this case)
    StreamError(String),

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

    ContextWindowUpdated(ContextWindow),
    OpenUsageView(ContextWindow, Usage),

    // Providers overlay
    OpenProvidersView(Vec<ProviderConfig>),
    AddProvider(ProviderConfig, Option<StoredCredential>),
    RemoveProvider(ProviderAlias),

    // Model picker overlay
    OpenModelPicker(Vec<ModelSelection>),
    SelectModel(ModelSelection),

    // Side-effect signals intercepted by Runtime before update()
    SendMessage(Vec<InputToken>),
    InterruptStream,
    ExecuteCommand(std::sync::Arc<dyn crate::commands::Command>),
    Quit,
}
