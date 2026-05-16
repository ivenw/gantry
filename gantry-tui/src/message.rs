use crate::model::SessionStats;
use gantry_core::{
    AppEvent, ChatStreamItem, ContextWindow, InputToken, ModelSelection, PathSearchResult,
    ProviderAlias, ProviderConfig, SessionId, SessionInfo, SessionTree, SkillSearchResult,
    StoredCredential, StreamingError, Usage,
};

use crate::features::chat::ChatMessage;
use crate::features::command_picker::KnownCommand;

/// Pure model-update messages handled by `update()`.
///
/// These drive all state transitions in `Model`. `update()` may return a `Cmd` to request
/// a side effect from `Runtime`.
pub enum Msg {
    // Input
    KeyEvent(crossterm::event::KeyEvent),

    // Stream events from the agent
    StreamItem(Result<ChatStreamItem, StreamingError>),
    StreamDone(SessionStats),

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
    OpenSessionTree(SessionTree),
    ReloadMessages(Vec<ChatMessage>),
    ReloadMessagesWithInput(Vec<ChatMessage>, String),

    // Sessions browser
    OpenSessionsPicker(Vec<SessionInfo>, SessionId),
    SessionLoaded {
        session_id: SessionId,
        messages: Vec<ChatMessage>,
        session_stats: SessionStats,
    },

    OpenUsageState(ContextWindow, Usage),

    // Providers overlay
    OpenProviderConfig(Vec<ProviderConfig>),

    // Model picker overlay
    /// Opens the model picker with a previously cached list.
    OpenModelPicker(Vec<ModelSelection>),
    /// Fresh fetch result: caches the list and opens the picker.
    ModelsFetched(Vec<ModelSelection>),

    // Attachment picker results (produced by Runtime after a search)
    SetPathPickerResults(Vec<PathSearchResult>),
    SetSkillPickerResults(Vec<SkillSearchResult>),

    // Activate the path or skill attachment picker with initial results.
    ActivatePathPicker(Vec<PathSearchResult>),
    ActivateSkillPicker(Vec<SkillSearchResult>),

    // Provider mutations completed; carries the refreshed provider list.
    ProviderAdded(Vec<ProviderConfig>),
    ProviderRemoved(Vec<ProviderConfig>),
    ProviderAddFailed(String),

    // Model selection applied to the app.
    ModelSelected(ModelSelection),

    // Transitions stream state to Active and opens a streaming message slot.
    StartStream,

    // Transitions stream state to Interrupted and flushes any buffered content.
    CancelStream,
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
