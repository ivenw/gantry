use std::time::{Duration, Instant};

use gantry_core::{
    ContextWindow, InputToken, ModelSelection, PathSearchResult, SessionId, SessionInfo,
    SessionTree, SkillSearchResult,
};

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

use crate::chat::ChatModel;
use crate::input::{AttachmentPicker, AttachmentPickerKind, InputModel};
use crate::model_picker::{ModelPickerView, format_context_length};
use crate::sessions::SessionsView;
use crate::tree::{TreeView, branch_rows};

/// The editing sub-mode active when no overlay is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Navigation mode. Typing opens the command picker; j/k scroll the chat.
    Normal,
    /// Text entry mode. Keys are forwarded to the input buffer.
    Insert,
}

/// The active overlay occupying the input area, or `Chat` when none is open.
pub enum InputOverlay {
    /// No overlay; the input buffer is active in the given mode.
    Chat(Mode),
    CommandPicker(crate::command_picker::CommandPicker),
    ModelPicker(crate::model_picker::ModelPickerView),
    AttachmentPicker(crate::input::AttachmentPicker),
    UsageView(crate::usage::UsageView),
    SessionsView(crate::sessions::SessionsView),
    TreeView(crate::tree::TreeView),
    Providers(crate::providers::ProvidersView),
}

pub struct Model {
    pub session_id: Option<SessionId>,
    pub selection: Option<ModelSelection>,
    pub overlay: InputOverlay,
    pub chat: ChatModel,
    pub input: InputModel,
    pub project_path: std::path::PathBuf,
    pub cwd: std::path::PathBuf,
    pub status_message: Option<String>,
    pub stream: StreamState,
    /// Context window snapshot from the most recently completed stream.
    pub context_window: Option<ContextWindow>,
    /// Cached model list fetched on first open of the model picker.
    pub cached_models: Option<Vec<ModelSelection>>,
}

impl Model {
    pub fn new() -> Self {
        Self {
            session_id: None,
            selection: None,
            overlay: InputOverlay::Chat(Mode::Normal),
            chat: ChatModel::new(),
            input: InputModel::new(),
            project_path: std::path::PathBuf::new(),
            cwd: std::path::PathBuf::new(),
            status_message: None,
            stream: StreamState::Idle,
            context_window: None,
            cached_models: None,
        }
    }

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

    /// Transitions stream state to `Done`, capturing elapsed duration and closing the chat slot.
    pub fn finish_stream(&mut self) {
        let duration = match self.stream {
            StreamState::Active { started_at } => started_at.elapsed(),
            _ => Duration::ZERO,
        };
        self.stream = StreamState::Done { duration };
        self.chat.finish_streaming();
    }

    /// Transitions stream state to `Interrupted`, capturing elapsed duration.
    ///
    /// Returns the streaming text that was in progress, if any, so the caller can restore it.
    pub fn cancel_stream(&mut self) -> Option<String> {
        let duration = match self.stream {
            StreamState::Active { started_at } => started_at.elapsed(),
            _ => Duration::ZERO,
        };
        self.stream = StreamState::Interrupted { duration };
        self.chat.cancel_streaming()
    }

    /// Opens the path attachment picker, inserting `+` into the input to seed the filter display.
    pub fn activate_path_picker(&mut self, results: Vec<PathSearchResult>) {
        self.input.insert('+');
        self.overlay = InputOverlay::AttachmentPicker(AttachmentPicker::new_path(results));
    }

    /// Opens the skill attachment picker, inserting `/` into the input to seed the filter display.
    pub fn activate_skill_picker(&mut self, results: Vec<SkillSearchResult>) {
        self.input.insert('/');
        self.overlay = InputOverlay::AttachmentPicker(AttachmentPicker::new_skill(results));
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

    /// Opens the model picker with the given list of available models.
    pub fn open_model_picker(&mut self, models: Vec<ModelSelection>) {
        let active_selection = self.selection.clone();
        let selected_idx = active_selection
            .as_ref()
            .and_then(|s| models.iter().position(|m| m == s))
            .unwrap_or(0);
        let model_col_width = models
            .iter()
            .map(|s| s.model_id.as_str().chars().count() as u16)
            .max()
            .unwrap_or(0);
        let provider_col_width = models
            .iter()
            .map(|s| s.provider_alias.as_str().chars().count() as u16)
            .max()
            .unwrap_or(0);
        let context_col_width = models
            .iter()
            .filter_map(|s| s.context_length)
            .map(|n| format_context_length(n).len() as u16)
            .max()
            .unwrap_or(0);
        let mut picker = ModelPickerView {
            models,
            filter: String::new(),
            selected_idx,
            active_selection,
            filtered: Vec::new(),
            model_col_width,
            provider_col_width,
            context_col_width,
        };
        picker.refilter();
        self.overlay = InputOverlay::ModelPicker(picker);
    }

    /// Returns the currently highlighted model selection in the model picker, if any.
    pub fn selected_model_in_picker(&self) -> Option<ModelSelection> {
        if let InputOverlay::ModelPicker(ref mv) = self.overlay {
            mv.filtered
                .get(mv.selected_idx)
                .map(|e| e.selection.clone())
        } else {
            None
        }
    }

    /// Opens the sessions browser, pre-selecting the currently active session.
    pub fn open_sessions_view(&mut self, sessions: Vec<SessionInfo>, active_session_id: SessionId) {
        let selected_idx = sessions
            .iter()
            .rposition(|s| s.id == active_session_id)
            .unwrap_or(sessions.len().saturating_sub(1));
        self.overlay = InputOverlay::SessionsView(SessionsView {
            sessions,
            selected_idx,
            active_session_id,
        });
    }

    /// Returns the session highlighted in the sessions browser, if any.
    pub fn selected_session(&self) -> Option<&SessionInfo> {
        if let InputOverlay::SessionsView(ref sv) = self.overlay {
            sv.sessions.get(sv.selected_idx)
        } else {
            None
        }
    }

    /// Opens the session tree overlay, pre-selecting the current leaf node.
    pub fn open_tree_view(&mut self, tree: SessionTree) {
        let selected_idx = branch_rows(&tree.stem, 0)
            .iter()
            .position(|(b, _)| b.node.id == tree.current_leaf_id)
            .unwrap_or(0);
        self.overlay = InputOverlay::TreeView(TreeView {
            tree,
            selected_idx,
            scroll_offset: 0,
        });
    }
}
