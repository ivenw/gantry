mod update;
pub use update::update;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gantry_core::{
    ContextWindow, InputToken, ModelSelection, PathSearchResult, SessionId, SessionInfo,
    SessionTree, SkillSearchResult, Usage,
};

use crate::features::attachment_picker::{AttachmentPickerKind, AttachmentPickerState};
use crate::features::chat::{AttachmentLabel, ChatState};
use crate::features::input::InputState;
use crate::features::model_picker::ModelPickerState;
use crate::features::session_picker::SessionPickerState;
use crate::features::tree::{TreeState, branch_rows};

pub struct Model {
    session_id: Option<SessionId>,
    selection: Option<ModelSelection>,
    overlay: InputOverlay,
    chat: ChatState,
    input: InputState,
    project_path: PathBuf,
    project_name: String,
    cwd: PathBuf,
    status_message: Option<String>,
    stream: StreamState,
    session_stats: SessionStats,
    /// Cached model list fetched on first open of the model picker.
    cached_models: Option<Vec<ModelSelection>>,
}

/// Snapshot of token consumption and context window from the most recently completed stream.
///
/// Both fields are updated atomically so they always reflect the same checkpoint.
#[derive(Clone, Default)]
pub struct SessionStats {
    /// Context window fill at the end of the last stream, or `None` before the first stream completes.
    pub context_window: Option<ContextWindow>,
    /// Cumulative token usage for the session. Zero before the first stream completes.
    pub usage: Usage,
}

impl Model {
    /// Creates a new model with the given initial application state.
    pub fn new(
        session_id: Option<SessionId>,
        messages: Vec<crate::features::chat::ChatMessage>,
        selection: Option<ModelSelection>,
        project_path: PathBuf,
        project_name: String,
        session_stats: SessionStats,
        cwd: PathBuf,
    ) -> Self {
        let mut chat = ChatState::new();
        chat.messages = messages;
        Self {
            session_id,
            selection,
            overlay: InputOverlay::Input(Mode::Normal),
            chat,
            input: InputState::new(),
            project_path,
            project_name,
            cwd,
            status_message: None,
            stream: StreamState::Idle,
            session_stats,
            cached_models: None,
        }
    }

    // ── Read accessors ────────────────────────────────────────────────────────

    /// Returns the active overlay.
    pub fn overlay(&self) -> &InputOverlay {
        &self.overlay
    }

    /// Returns the chat state.
    pub fn chat(&self) -> &ChatState {
        &self.chat
    }

    /// Returns the input state.
    pub fn input(&self) -> &InputState {
        &self.input
    }

    /// Returns the current stream state.
    pub fn stream(&self) -> &StreamState {
        &self.stream
    }

    /// Returns the status message, if any.
    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }

    /// Returns the session stats snapshot from the most recently completed stream.
    pub fn session_stats(&self) -> &SessionStats {
        &self.session_stats
    }

    /// Returns the project name.
    pub fn project_name(&self) -> &str {
        &self.project_name
    }

    /// Returns the project root path.
    pub fn project_path(&self) -> &PathBuf {
        &self.project_path
    }

    /// Returns the current working directory.
    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }

    /// Returns the active model selection.
    pub fn selection(&self) -> Option<&ModelSelection> {
        self.selection.as_ref()
    }

    /// Returns true if the active attachment picker is a skill picker.
    pub fn is_skill_attachment_picker_active(&self) -> bool {
        matches!(
            &self.overlay,
            InputOverlay::AttachmentPicker(p)
                if matches!(p.kind, AttachmentPickerKind::Skill(_))
        )
    }

    /// Returns the cached model list, if it has been fetched.
    pub fn cached_models(&self) -> Option<&[ModelSelection]> {
        self.cached_models.as_deref()
    }

    // ── Stream transitions ────────────────────────────────────────────────────

    /// Returns true if a stream is currently in progress.
    pub fn is_streaming(&self) -> bool {
        matches!(self.stream, StreamState::Active { .. })
    }

    /// Transitions stream state to `Idle`, collapsing the agent statusline.
    pub fn reset_stream(&mut self) {
        self.stream = StreamState::Idle;
    }

    /// Transitions stream state to `Active` and opens a streaming message slot in the chat.
    pub fn start_stream(&mut self) {
        self.stream = StreamState::Active {
            started_at: Instant::now(),
        };
        self.chat.start_streaming_message();
    }

    fn finish_stream(&mut self) {
        let duration = match self.stream {
            StreamState::Active { started_at } => started_at.elapsed(),
            _ => Duration::ZERO,
        };
        self.stream = StreamState::Done { duration };
        self.chat.finish_streaming();
    }

    /// Transitions stream state to `Interrupted`, capturing elapsed duration, and flushes
    /// any buffered content to the visible message so the partial response remains readable.
    pub fn interrupt_stream(&mut self) {
        let duration = match self.stream {
            StreamState::Active { started_at } => started_at.elapsed(),
            _ => Duration::ZERO,
        };
        self.stream = StreamState::Interrupted { duration };
        self.chat.interrupt_streaming();
    }

    // ── Attachment picker ─────────────────────────────────────────────────────

    /// Opens the path attachment picker, inserting `+` into the input to seed the filter display.
    pub fn activate_path_picker(&mut self, results: Vec<PathSearchResult>) {
        self.input.insert('+');
        self.overlay = InputOverlay::AttachmentPicker(AttachmentPickerState::new_path(results));
    }

    /// Opens the skill attachment picker, inserting `/` into the input to seed the filter display.
    pub fn activate_skill_picker(&mut self, results: Vec<SkillSearchResult>) {
        self.input.insert('/');
        self.overlay = InputOverlay::AttachmentPicker(AttachmentPickerState::new_skill(results));
    }

    /// Appends a character to the attachment picker filter string and mirrors it into the input.
    ///
    /// Search results are replaced by the caller via `Msg::RefineAttachmentPicker` after
    /// the new query is known; this method only updates the filter string and input.
    pub fn attachment_picker_filter_push(&mut self, c: char) {
        if let InputOverlay::AttachmentPicker(ref mut picker) = self.overlay {
            picker.filter.push(c);
            picker.selected_idx = 0;
        }
        self.input.insert(c);
    }

    /// Clears the attachment picker filter string and removes all filter characters from the input.
    pub fn attachment_picker_filter_clear(&mut self) {
        if let InputOverlay::AttachmentPicker(ref mut picker) = self.overlay {
            let len = picker.filter.len();
            picker.filter.clear();
            picker.selected_idx = 0;
            for _ in 0..len {
                self.input.delete_before_cursor();
            }
        }
    }

    /// Removes the last character from the attachment picker filter string and from the input.
    ///
    /// Returns `false` when the filter was already empty, signalling the caller to close the picker.
    pub fn attachment_picker_filter_pop(&mut self) -> bool {
        if let InputOverlay::AttachmentPicker(ref mut picker) = self.overlay {
            if picker.filter.is_empty() {
                // Remove the sigil from the input.
                self.input.delete_before_cursor();
                return false;
            }
            picker.filter.pop();
            picker.selected_idx = 0;
            self.input.delete_before_cursor();
            true
        } else {
            false
        }
    }

    /// Returns the current filter string of the attachment picker.
    pub fn attachment_picker_filter(&self) -> Option<&str> {
        if let InputOverlay::AttachmentPicker(ref picker) = self.overlay {
            Some(picker.filter.as_str())
        } else {
            None
        }
    }

    /// Returns the selected attachment token from the active attachment picker, if any.
    pub fn selected_attachment(&self) -> Option<InputToken> {
        let InputOverlay::AttachmentPicker(ref picker) = self.overlay else {
            return None;
        };
        match &picker.kind {
            AttachmentPickerKind::Path(results) => {
                let path = &results.get(picker.selected_idx)?.path;
                Some(InputToken::Path(path.clone()))
            }
            AttachmentPickerKind::Skill(results) => {
                let skill = &results.get(picker.selected_idx)?.skill;
                Some(InputToken::Skill {
                    name: skill.metadata.name.clone(),
                    path: skill.skill_file.clone(),
                })
            }
        }
    }

    // ── Model picker ──────────────────────────────────────────────────────────

    /// Opens the model picker with the given list of available models, pre-selecting the active model.
    pub fn open_model_picker(&mut self, models: Vec<ModelSelection>) {
        let active_selection = self.selection.clone();
        let active_ref = active_selection.clone();
        let mut picker = ModelPickerState::new(models, active_selection);
        // Pre-select the currently active model if it is in the list.
        if let Some(active) = active_ref {
            picker.picker.set_selected(|e| e.selection == active);
        }
        self.overlay = InputOverlay::ModelPicker(picker);
    }

    /// Returns the currently highlighted model selection in the model picker, if any.
    pub fn selected_model_in_picker(&self) -> Option<ModelSelection> {
        if let InputOverlay::ModelPicker(ref mv) = self.overlay {
            mv.picker.selected().map(|e| e.selection.clone())
        } else {
            None
        }
    }

    // ── Session picker ────────────────────────────────────────────────────────

    /// Opens the sessions browser, pre-selecting the currently active session.
    pub fn open_sessions_picker(
        &mut self,
        sessions: Vec<SessionInfo>,
        active_session_id: SessionId,
    ) {
        let mut state = SessionPickerState::new(sessions, active_session_id);
        // Pre-select the active session (last match, so the most recent active session wins).
        state
            .picker
            .set_selected_last(|s| s.id == state.active_session_id);
        self.overlay = InputOverlay::SessionPicker(state);
    }

    /// Returns the session highlighted in the sessions browser, if any.
    pub fn selected_session(&self) -> Option<&SessionInfo> {
        if let InputOverlay::SessionPicker(ref sv) = self.overlay {
            sv.picker.selected()
        } else {
            None
        }
    }

    // ── Session lifecycle ─────────────────────────────────────────────────────

    /// Resets all session-scoped state for a new session.
    pub fn reset_session(&mut self) {
        self.chat.reset();
        self.status_message = None;
        self.reset_stream();
        self.session_stats = SessionStats::default();
    }

    /// Replaces all session-scoped state with the given session's data.
    pub fn load_session(
        &mut self,
        session_id: SessionId,
        messages: Vec<crate::features::chat::ChatMessage>,
        session_stats: SessionStats,
    ) {
        self.session_id = Some(session_id);
        self.chat.messages = messages;
        self.chat.scroll_offset = 0;
        self.chat.user_is_scrolling = false;
        self.session_stats = session_stats;
        self.reset_stream();
        self.status_message = None;
    }

    /// Replaces the visible messages and returns to normal input mode.
    pub fn reload_messages(&mut self, messages: Vec<crate::features::chat::ChatMessage>) {
        self.chat.messages = messages;
        self.chat.scroll_offset = 0;
        self.chat.user_is_scrolling = false;
        self.overlay = InputOverlay::Input(Mode::Normal);
    }

    /// Replaces the visible messages, restores the input buffer, and returns to normal input mode.
    pub fn reload_messages_with_input(
        &mut self,
        messages: Vec<crate::features::chat::ChatMessage>,
        input: String,
    ) {
        self.chat.messages = messages;
        self.chat.scroll_offset = 0;
        self.chat.user_is_scrolling = false;
        self.input.set_text(input);
        self.overlay = InputOverlay::Input(Mode::Normal);
    }

    /// Completes the stream successfully, scrolling chat to the bottom.
    ///
    /// No-ops if the stream was interrupted, so a late done signal does not overwrite
    /// the interrupted state.
    pub fn complete_stream(&mut self) {
        if matches!(self.stream, StreamState::Interrupted { .. }) {
            return;
        }
        self.finish_stream();
        self.chat.scroll_to_bottom();
    }

    /// Updates the cached metrics snapshot from the most recently completed turn.
    pub fn update_metrics(&mut self, stats: SessionStats) {
        self.session_stats = stats;
    }

    /// Fails the stream, rolling back optimistic messages, restoring the input, and recording the error.
    pub fn fail_stream(&mut self, error: String) {
        if let Some(tokens) = self.chat.rollback_streaming() {
            self.input.restore_tokens(tokens);
        }
        self.interrupt_stream();
        self.status_message = Some(error);
    }

    // ── Provider config ───────────────────────────────────────────────────────

    /// Invalidates the cached model list and opens the provider config overlay.
    pub fn open_provider_config(&mut self, providers: Vec<gantry_core::ProviderConfig>) {
        self.cached_models = None;
        self.overlay =
            InputOverlay::ProviderConfig(crate::features::provider_config::ProvidersConfigState {
                providers,
                sub: crate::features::provider_config::ProvidersSubView::List { selected_idx: 0 },
            });
    }

    // ── Input submission ──────────────────────────────────────────────────────

    /// Clears the input buffer, stages the message in chat, and resets scroll state.
    ///
    /// Returns the input tokens to be sent, or `None` if submission should be blocked.
    /// Submission is blocked when the input is blank, a stream is active, no model is selected,
    /// or the input looks like an unknown slash command.
    pub fn submit_message(&mut self) -> Option<Vec<InputToken>> {
        if self.status_message.is_some() {
            self.status_message = None;
            return None;
        }

        if self.input.is_blank() || self.is_streaming() {
            return None;
        }

        if self.selection.is_none() {
            self.status_message = Some("No model selected".to_string());
            return None;
        }

        let display = self.input.raw_display(&self.project_path);
        if display.starts_with('/') {
            let filter = display.strip_prefix('/').unwrap_or("");
            let has_match = crate::features::command_picker::KnownCommand::ALL
                .iter()
                .any(|k| k.name().starts_with(filter));
            if !has_match {
                self.input.clear();
                return None;
            }
        }

        let tokens = self.input.tokens.clone();
        let display = self.input.raw_display(&self.project_path);
        self.input.clear();
        let labels = AttachmentLabel::from_tokens(&tokens, &self.project_path);
        self.chat.add_user_message(display, labels, tokens.clone());
        self.chat.scroll_offset = 0;
        self.chat.user_is_scrolling = false;
        Some(tokens)
    }

    // ── Session tree ──────────────────────────────────────────────────────────

    /// Opens the session tree overlay, pre-selecting the current leaf node.
    pub fn open_session_tree(&mut self, tree: SessionTree) {
        let selected_idx = branch_rows(&tree.stem, 0)
            .iter()
            .position(|(b, _)| b.id == tree.current_leaf_id)
            .unwrap_or(0);
        self.overlay = InputOverlay::Tree(TreeState {
            tree,
            selected_idx,
            scroll_offset: 0,
        });
    }
}

/// The active overlay occupying the input area, or `Input` when none is open.
pub enum InputOverlay {
    /// No overlay; the input buffer is active in the given mode.
    Input(Mode),
    CommandPicker(crate::features::command_picker::CommandPickerState),
    ModelPicker(crate::features::model_picker::ModelPickerState),
    AttachmentPicker(crate::features::attachment_picker::AttachmentPickerState),
    Usage(gantry_core::ContextWindow),
    SessionPicker(crate::features::session_picker::SessionPickerState),
    Tree(crate::features::tree::TreeState),
    ProviderConfig(crate::features::provider_config::ProvidersConfigState),
}

/// The editing sub-mode active when no overlay is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Navigation mode. Typing opens the command picker; j/k scroll the chat.
    Normal,
    /// Text entry mode. Keys are forwarded to the input buffer.
    Insert,
}

/// The state of the agent stream.
#[derive(Debug, Clone)]
pub enum StreamState {
    /// No stream has started, or the previous stream state was cleared.
    Idle,
    /// A stream is currently in progress.
    Active { started_at: Instant },
    /// The stream was cancelled by the user.
    Interrupted { duration: Duration },
    /// The stream completed successfully.
    Done { duration: Duration },
}
