use std::time::{Duration, Instant};

use gantry_core::{
    Branch, ContextWindow, InputToken, ModelSelection, PathSearchResult, ProviderConfig, SessionId,
    SessionInfo, SessionTree, SkillSearchResult, Usage,
};

use crate::chat::ChatModel;
use crate::command_picker::{CommandEntry, CommandPicker};
use crate::input::{AttachmentPicker, AttachmentPickerKind, InputModel};
use crate::model_picker::{ModelPickerView, format_context_length};
use crate::providers::{ProvidersSubView, ProvidersView};
use crate::sessions::SessionsView;
use crate::tree::{TreeView, branch_rows};
use crate::usage::UsageView;

/// The top-level editing mode, analogous to Vim's modal editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Navigation/command mode. Typing does not enter text into the input buffer.
    Normal,
    /// Text entry mode. Keys are forwarded to the input buffer.
    Insert,
}

pub struct Model {
    pub session_id: Option<SessionId>,
    pub selection: Option<ModelSelection>,
    pub mode: InputMode,
    pub chat: ChatModel,
    pub input: InputModel,
    pub project_path: std::path::PathBuf,
    pub cwd: std::path::PathBuf,
    pub command_picker: Option<CommandPicker>,
    pub attachment_picker: Option<AttachmentPicker>,
    pub sessions_view: Option<SessionsView>,
    pub tree_view: Option<TreeView>,
    pub providers_view: Option<ProvidersView>,
    pub model_picker_view: Option<ModelPickerView>,
    pub usage_view: Option<UsageView>,
    pub status_message: Option<String>,
    /// Wall-clock time when the current or most recent stream started.
    stream_started_at: Option<Instant>,
    /// Elapsed duration of the most recently completed stream.
    stream_duration: Option<Duration>,
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
            mode: InputMode::Normal,
            chat: ChatModel::new(),
            input: InputModel::new(),
            project_path: std::path::PathBuf::new(),
            cwd: std::path::PathBuf::new(),
            command_picker: None,
            attachment_picker: None,
            sessions_view: None,
            tree_view: None,
            providers_view: None,
            model_picker_view: None,
            usage_view: None,
            status_message: None,
            stream_started_at: None,
            stream_duration: None,
            context_window: None,
            cached_models: None,
        }
    }

    pub fn is_streaming(&self) -> bool {
        self.chat.streaming_content.is_some()
    }

    /// Returns the wall-clock time when the current stream started, if one is in progress.
    pub fn stream_started_at(&self) -> Option<Instant> {
        self.stream_started_at
    }

    /// Returns the elapsed duration of the most recently completed stream, if any.
    pub fn stream_duration(&self) -> Option<Duration> {
        self.stream_duration
    }

    /// Clears all stream state, collapsing the agent statusline.
    pub fn reset_stream(&mut self) {
        self.stream_started_at = None;
        self.stream_duration = None;
    }

    /// Begins a new stream, resetting the elapsed timer.
    pub fn start_stream(&mut self) {
        self.stream_started_at = Some(Instant::now());
        self.stream_duration = None;
        self.chat.start_streaming_message();
    }

    /// Finalises a completed stream, capturing the elapsed duration.
    pub fn finish_stream(&mut self) {
        self.stream_duration = self.stream_started_at.map(|t| t.elapsed());
        self.stream_started_at = None;
        self.chat.finish_streaming();
    }

    /// Cancels an in-progress stream, capturing the elapsed duration.
    ///
    /// Returns the streaming text that was in progress, if any, so the caller can restore it.
    pub fn cancel_stream(&mut self) -> Option<String> {
        self.stream_duration = self.stream_started_at.map(|t| t.elapsed());
        self.stream_started_at = None;
        self.chat.cancel_streaming()
    }

    pub fn is_command_picker_active(&self) -> bool {
        self.command_picker.is_some()
    }

    // Command picker mutations
    pub fn activate_command_picker(&mut self, commands: Vec<CommandEntry>) {
        let cmd_col_width = commands
            .iter()
            .map(|c| c.name.chars().count() as u16)
            .max()
            .unwrap_or(0);
        let mut picker = CommandPicker {
            commands,
            filter: String::new(),
            selected_idx: 0,
            filtered: Vec::new(),
            cmd_col_width,
        };
        picker.refilter();
        self.command_picker = Some(picker);
    }

    pub fn deactivate_command_picker(&mut self) {
        self.command_picker = None;
    }

    /// Appends a character to the command picker's filter string.
    pub fn command_picker_filter_push(&mut self, c: char) {
        if let Some(ref mut picker) = self.command_picker {
            picker.filter.push(c);
            picker.selected_idx = 0;
            picker.refilter();
        }
    }

    /// Removes the last character from the command picker's filter string.
    pub fn command_picker_filter_pop(&mut self) {
        if let Some(ref mut picker) = self.command_picker {
            picker.filter.pop();
            picker.selected_idx = 0;
            picker.refilter();
        }
    }

    /// Moves the command picker cursor up, wrapping from the first to the last entry.
    pub fn move_command_selection_up(&mut self) {
        if let Some(ref mut picker) = self.command_picker {
            let count = picker.filtered.len();
            if count > 0 {
                picker.selected_idx = picker.selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
        }
    }

    /// Moves the command picker cursor down, wrapping from the last to the first entry.
    pub fn move_command_selection_down(&mut self) {
        if let Some(ref mut picker) = self.command_picker {
            let count = picker.filtered.len();
            if count > 0 {
                picker.selected_idx = (picker.selected_idx + 1) % count;
            }
        }
    }

    /// Returns the currently highlighted command in the command picker, if any.
    pub fn selected_command(&self) -> Option<CommandEntry> {
        self.command_picker
            .as_ref()
            .and_then(|p| p.filtered.get(p.selected_idx).cloned())
    }

    // Attachment picker mutations

    pub fn is_attachment_picker_active(&self) -> bool {
        self.attachment_picker.is_some()
    }

    /// Opens the path attachment picker, inserting `+` into the input to seed the filter display.
    pub fn activate_path_picker(&mut self, results: Vec<PathSearchResult>) {
        self.input.insert('+');
        self.attachment_picker = Some(AttachmentPicker::new_path(results));
    }

    /// Opens the skill attachment picker, inserting `/` into the input to seed the filter display.
    pub fn activate_skill_picker(&mut self, results: Vec<SkillSearchResult>) {
        self.input.insert('/');
        self.attachment_picker = Some(AttachmentPicker::new_skill(results));
    }

    /// Closes the attachment picker, leaving the typed sigil and filter text in the input as-is.
    pub fn deactivate_attachment_picker(&mut self) {
        self.attachment_picker = None;
    }

    /// Appends a character to the attachment picker filter string and mirrors it into the input.
    ///
    /// Search results are replaced by the caller via `Msg::RefineAttachmentPicker` after
    /// the new query is known; this method only updates the filter string and input.
    pub fn attachment_picker_filter_push(&mut self, c: char) {
        if let Some(ref mut picker) = self.attachment_picker {
            picker.filter.push(c);
            picker.selected_idx = 0;
        }
        self.input.insert(c);
    }

    /// Clears the attachment picker filter string and removes all filter characters from the input.
    pub fn attachment_picker_filter_clear(&mut self) {
        if let Some(ref mut picker) = self.attachment_picker {
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
        if let Some(ref mut picker) = self.attachment_picker {
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
        self.attachment_picker.as_ref().map(|p| p.filter.as_str())
    }

    pub fn move_attachment_selection_up(&mut self) {
        if let Some(ref mut picker) = self.attachment_picker {
            let count = picker.len();
            if count > 0 {
                picker.selected_idx = picker.selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
        }
    }

    pub fn move_attachment_selection_down(&mut self) {
        if let Some(ref mut picker) = self.attachment_picker {
            let count = picker.len();
            if count > 0 {
                picker.selected_idx = (picker.selected_idx + 1) % count;
            }
        }
    }

    /// Returns the selected attachment token, if any item is selected.
    pub fn selected_attachment(&self) -> Option<InputToken> {
        let picker = self.attachment_picker.as_ref()?;
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

    // Sessions view mutations

    pub fn is_sessions_view_active(&self) -> bool {
        self.sessions_view.is_some()
    }

    /// Opens the sessions browser, pre-selecting the currently active session.
    pub fn activate_sessions_view(
        &mut self,
        sessions: Vec<SessionInfo>,
        active_session_id: SessionId,
    ) {
        let selected_idx = sessions
            .iter()
            .rposition(|s| s.id == active_session_id)
            .unwrap_or(sessions.len().saturating_sub(1));
        self.sessions_view = Some(SessionsView {
            sessions,
            selected_idx,
            active_session_id,
        });
    }

    pub fn deactivate_sessions_view(&mut self) {
        self.sessions_view = None;
    }

    pub fn move_sessions_selection_up(&mut self) {
        if let Some(ref mut sv) = self.sessions_view
            && !sv.sessions.is_empty()
        {
            sv.selected_idx = sv
                .selected_idx
                .checked_sub(1)
                .unwrap_or(sv.sessions.len() - 1);
        }
    }

    pub fn move_sessions_selection_down(&mut self) {
        if let Some(ref mut sv) = self.sessions_view
            && !sv.sessions.is_empty()
        {
            sv.selected_idx = (sv.selected_idx + 1) % sv.sessions.len();
        }
    }

    /// Returns the session highlighted in the browser, if any.
    pub fn selected_session(&self) -> Option<&SessionInfo> {
        self.sessions_view
            .as_ref()
            .and_then(|sv| sv.sessions.get(sv.selected_idx))
    }

    // Tree view mutations

    pub fn is_tree_view_active(&self) -> bool {
        self.tree_view.is_some()
    }

    pub fn activate_tree_view(&mut self, tree: SessionTree) {
        let selected_idx = branch_rows(&tree.stem, 0)
            .iter()
            .position(|(b, _)| b.node.id == tree.current_leaf_id)
            .unwrap_or(0);
        self.tree_view = Some(TreeView {
            tree,
            selected_idx,
            scroll_offset: 0,
        });
    }

    pub fn deactivate_tree_view(&mut self) {
        self.tree_view = None;
    }

    pub fn move_tree_selection_up(&mut self) {
        if let Some(ref mut tv) = self.tree_view {
            tv.selected_idx = tv.selected_idx.saturating_sub(1);
        }
    }

    pub fn move_tree_selection_down(&mut self) {
        if let Some(ref mut tv) = self.tree_view {
            let count = branch_rows(&tv.tree.stem, 0).len();
            if count > 0 {
                tv.selected_idx = (tv.selected_idx + 1).min(count - 1);
            }
        }
    }

    pub fn is_providers_view_active(&self) -> bool {
        self.providers_view.is_some()
    }

    pub fn activate_providers_view(&mut self, providers: Vec<ProviderConfig>) {
        self.providers_view = Some(ProvidersView {
            providers,
            sub: ProvidersSubView::List { selected_idx: 0 },
        });
    }

    pub fn deactivate_providers_view(&mut self) {
        self.providers_view = None;
    }

    pub fn is_model_picker_active(&self) -> bool {
        self.model_picker_view.is_some()
    }

    /// Opens the model picker with the given list of available models.
    pub fn activate_model_picker_view(&mut self, models: Vec<ModelSelection>) {
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
        self.model_picker_view = Some(picker);
    }

    pub fn deactivate_model_picker_view(&mut self) {
        self.model_picker_view = None;
    }

    /// Returns `true` if the context window usage overlay is currently shown.
    pub fn is_usage_view_active(&self) -> bool {
        self.usage_view.is_some()
    }

    /// Activates the context window usage overlay with the given snapshot.
    pub fn activate_usage_view(&mut self, context_window: ContextWindow, consumption: Usage) {
        self.usage_view = Some(UsageView {
            context_window,
            consumption,
        });
    }

    /// Closes the context window usage overlay.
    pub fn deactivate_usage_view(&mut self) {
        self.usage_view = None;
    }

    /// Appends a character to the model picker filter string and resets the cursor to the top.
    pub fn model_picker_filter_push(&mut self, c: char) {
        if let Some(ref mut mv) = self.model_picker_view {
            mv.filter.push(c);
            mv.selected_idx = 0;
            mv.refilter();
        }
    }

    /// Removes the last character from the model picker filter string and resets the cursor.
    pub fn model_picker_filter_pop(&mut self) {
        if let Some(ref mut mv) = self.model_picker_view {
            mv.filter.pop();
            mv.selected_idx = 0;
            mv.refilter();
        }
    }

    /// Moves the model picker cursor up, wrapping from the first to the last entry.
    pub fn move_model_picker_selection_up(&mut self) {
        if let Some(ref mut mv) = self.model_picker_view {
            let count = mv.filtered.len();
            if count > 0 {
                mv.selected_idx = mv.selected_idx.checked_sub(1).unwrap_or(count - 1);
            }
        }
    }

    /// Moves the model picker cursor down, wrapping from the last to the first entry.
    pub fn move_model_picker_selection_down(&mut self) {
        if let Some(ref mut mv) = self.model_picker_view {
            let count = mv.filtered.len();
            if count > 0 {
                mv.selected_idx = (mv.selected_idx + 1) % count;
            }
        }
    }

    /// Returns the currently highlighted model selection in the model picker, if any.
    pub fn selected_model_in_picker(&self) -> Option<ModelSelection> {
        self.model_picker_view.as_ref().and_then(|mv| {
            mv.filtered
                .get(mv.selected_idx)
                .map(|e| e.selection.clone())
        })
    }

    pub fn selected_tree_node(&self) -> Option<&Branch> {
        self.tree_view
            .as_ref()
            .and_then(|tv| {
                branch_rows(&tv.tree.stem, 0)
                    .into_iter()
                    .nth(tv.selected_idx)
            })
            .map(|(n, _)| n)
    }
}
