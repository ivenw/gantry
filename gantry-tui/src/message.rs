use gantry_core::{
    AppEvent, ChatStreamItem, ContextWindow, InputToken, ModelSelection, PathSearchResult,
    ProviderAlias, ProviderConfig, SessionId, SessionInfo, SessionTree, SkillSearchResult,
    StoredCredential, StreamingError, Usage,
};

use crate::chat::ChatMessage;
use crate::commands::KnownCommand;

/// Pure model-update messages handled by `update()`.
///
/// These drive all state transitions in `Model`. `update()` may return a `Cmd` to request
/// a side effect from `Runtime`.
pub enum Msg {
    // Input
    Key(crossterm::event::KeyEvent),

    // Stream events from the agent
    StreamItem(Result<ChatStreamItem, StreamingError>),
    StreamDone,

    // Out-of-band tool events
    AppEvent(AppEvent),

    // Streaming error (stream task failed; StreamDone is not sent in this case)
    StreamError(String),

    // Command results
    SetStatus(String),
    /// Applied after `Cmd::NewSession` creates a fresh session in the app.
    SessionCreated,

    // Scroll the chat window (positive = up, negative = down)
    ScrollChat(i32),

    // Tree view
    OpenTreeView(SessionTree),
    ReloadMessages(Vec<ChatMessage>),
    ReloadMessagesWithInput(Vec<ChatMessage>, String),

    // Sessions browser
    OpenSessionsState(Vec<SessionInfo>, SessionId),

    ContextWindowUpdated(ContextWindow),
    OpenUsageState(ContextWindow, Usage),

    // Providers overlay
    OpenProvidersState(Vec<ProviderConfig>),

    // Model picker overlay
    OpenModelPicker(Vec<ModelSelection>),

    // Attachment picker results (produced by Runtime after a search)
    SetPathPickerResults(Vec<PathSearchResult>),
    SetSkillPickerResults(Vec<SkillSearchResult>),
}

/// Side-effect commands returned by `update()` and executed by `Runtime`.
///
/// A `Cmd` requests async I/O or app-level mutations that cannot happen inside the pure
/// `update()` function. `Runtime` executes each `Cmd` and may send follow-up `Msg` values
/// back into the event loop.
pub enum Cmd {
    // Trigger an agent stream with the given tokens.
    SendMessage(Vec<InputToken>),

    // Open the path attachment picker with results for the given query.
    OpenPathPicker(String),

    // Open the skill attachment picker with results for the given query.
    OpenSkillPicker(String),

    // Re-run the attachment picker search with an updated query.
    RefineAttachmentPicker(String),

    /// Creates a new session in the app and then sends `Msg::SessionCreated`.
    NewSession,
    InterruptStream,
    RunCommand(KnownCommand),

    // Session branching
    BranchTo(String),
    BranchToWithInput {
        branch_id: String,
        input: String,
    },
    ResumeSession(SessionId),

    // Provider mutations
    AddProvider(ProviderConfig, Option<StoredCredential>),
    RemoveProvider(ProviderAlias),

    // Model selection (applied to the app, not just the model)
    SelectModel(ModelSelection),

    Quit,
}
