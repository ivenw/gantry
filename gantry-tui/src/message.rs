use gantry_core::{
    ChatStreamItem, ContextWindow, InputToken, ModelSelection, PathSearchResult, ProviderAlias,
    ProviderConfig, SessionId, SessionInfo, SessionTree, SkillSearchResult, StoredCredential,
    StreamingError, Usage,
};

use crate::commands::KnownCommand;
use crate::model::ChatMessage;

pub enum Msg {
    // Input
    Key(crossterm::event::KeyEvent),

    // Stream events from the agent
    StreamItem(Result<ChatStreamItem, StreamingError>),
    StreamDone,

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
    /// Open the path attachment picker with results for the given query.
    OpenPathPicker(String),
    /// Open the skill attachment picker with results for the given query.
    OpenSkillPicker(String),
    /// Re-run the attachment picker search with an updated query.
    RefineAttachmentPicker(String),
    /// Populate the attachment picker with fresh path results (produced by Runtime).
    SetPathPickerResults(Vec<PathSearchResult>),
    /// Populate the attachment picker with fresh skill results (produced by Runtime).
    SetSkillPickerResults(Vec<SkillSearchResult>),
    InterruptStream,
    RunCommand(KnownCommand),
    Quit,
}
